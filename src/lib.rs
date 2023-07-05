use std::{net::{TcpListener, TcpStream, Shutdown}, io::{Read, ErrorKind, Write}};

use game::GameServerState;
mod game;

fn handle_client(stream : &mut TcpStream, game_server_state : &mut GameServerState) -> bool {
    let _ = stream.set_nonblocking(true)
        .expect("Non blocking sockets must be supported");
    loop {
        // Get the client prompt for the current stream's state
        let prompt = game_server_state.get_client_prompt(stream);
        let user_state = game_server_state.user_state.get(&stream.peer_addr().unwrap());
        if user_state.is_none() || user_state.is_some_and(|x| x.prev_prompt != prompt) {
            match write(stream, &prompt) {
                Ok(_) => {
                    game::get_user_state(&mut game_server_state.user_state, stream).prev_prompt = prompt;
                },
                Err(_) => {
                    println!("Unrecoverable write error encountered, dropping connection to {}", stream.peer_addr().unwrap());
                    return false;
                }
            }
        }
        // based on the returned value, get the response and run the logic for that
        match read_until_block(stream, 10) {
            Ok(line) => {
                game_server_state.client_logic(stream, Some(line));
            },
            Err(e) if e.error_type == ReadLineErrorType::StringParsing => {
                println!("String parsing error encountered");
                continue;
            },
            Err(e) if e.error_type == ReadLineErrorType::WouldBlock => {
                game_server_state.client_logic(stream, None);
                break;
            },
            Err(e) if e.error_type == ReadLineErrorType::Disconnected => {
                println!("Disconnected from {}", stream.peer_addr().unwrap());
                game_server_state.client_disconnect(stream);
                return false;
            }
            Err(_) => {
                game_server_state.client_disconnect(stream);
                println!("Unrecoverable error encountered, dropping connection to {}", stream.peer_addr().unwrap());
                return false;
            }
        }
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadLineErrorType {
    StringParsing,
    Unrecoverable,
    WouldBlock,
    Disconnected
}

#[derive(Debug, Clone)]
pub struct ReadLineError {
    error_type : ReadLineErrorType
}

/// Reads from the given socket until it would block
/// requires the input socket to be non blocking
/// buf_size is the size of the buffer used when copying from the socket
pub fn read_until_block(stream : &mut TcpStream, buf_size : usize) -> Result<String, ReadLineError> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let mut buf = vec![0; buf_size];
        let read_size = match stream.read(&mut buf) {
            Ok(r) => r,
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                // if would block, then we either have an entire line
                // or there's no more data right now to grab
                if line.len() == 0 {
                    return Err(ReadLineError { error_type: ReadLineErrorType::WouldBlock });
                }
                return String::from_utf8(line)
                    .map_err(|_| ReadLineError { error_type: ReadLineErrorType::StringParsing })
                    .map(|line| {
                        println!("{} <- {}: {:?}", 
                            stream.local_addr().unwrap(), 
                            stream.peer_addr().unwrap(),
                            line.trim_end_matches('\n').trim_end_matches('\r'));
                        line
                    });
            },
            Err(_) => return Err(ReadLineError { error_type: ReadLineErrorType::Unrecoverable })
        };
        if read_size == 0 {
            return Err(ReadLineError { error_type: ReadLineErrorType::Disconnected })
        }
        line.extend_from_slice(&buf[..read_size]);
    }
}

pub fn write(stream : &mut TcpStream, line : &str) -> Result<(), std::io::Error> {
    println!("{} -> {}: {:?}", 
        stream.local_addr().unwrap(), 
        stream.peer_addr().unwrap(),
        line.trim_end_matches('\n').trim_end_matches('\r'));
    stream.write_all(line.as_bytes())
}

/// The event loop for the TCP server
/// Handles all the sockets connections and disconnections
pub fn event_loop(listener : TcpListener) -> std::io::Result<()> {
    let _ = listener.set_nonblocking(true)
        .expect("Non blocking sockets must be supported");

    let mut game_server_state = game::GameServerState::new();
    let mut open_streams = Vec::new();
    loop {
        // get incoming connections
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next() {
            match stream {
                Ok(stream) => {
                    println!("New connection {}", stream.peer_addr().unwrap()); 
                    open_streams.push(stream);
                },
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => return Err(e)
            }
        }
        // iterate through open streams and process
        open_streams.retain_mut(|stream| {
            let retain = handle_client(stream, &mut game_server_state);
            if !retain {
                let _ = stream.shutdown(Shutdown::Both);
            }
            retain
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{net::{TcpListener, TcpStream, Shutdown}, io::Write};
    use crate::{read_until_block, ReadLineErrorType};

    fn run_line_test(send_line : &str) {
        // create a listener
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        // create a client socket 
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        // get client connection from listener, make it non blocking
        let mut stream = listener.accept().unwrap().0;
        stream.set_nonblocking(true).unwrap();
        // make sure it connected correctly
        assert_eq!(stream.local_addr().unwrap(), client.peer_addr().unwrap());
        assert_eq!(stream.peer_addr().unwrap(), client.local_addr().unwrap());
        // send one client line
        client.write_all(send_line.as_bytes()).unwrap(); 
        client.flush().unwrap();
        // receive it
        loop {
            match read_until_block(&mut stream, 10) {
                Ok(recv_line) => {
                    // assert it's the same
                    assert_eq!(send_line, recv_line);
                    client.shutdown(Shutdown::Both).unwrap();
                    stream.shutdown(Shutdown::Both).unwrap();
                    return;
                },
                Err(e) if e.error_type == ReadLineErrorType::WouldBlock => {
                    continue
                },
                Err(e) => panic!("{:?}", e)
            }
        }
    }

    #[test]
    fn simple_read() {
        run_line_test("TEST ABC\r\n");
    }

    #[test]
    fn simple_read_no_crlf() {
        run_line_test("TEST ABC");
    }

    #[test]
    fn simple_read_lf() {
        run_line_test("TEST ABC\n");
    }

    #[test]
    fn simple_read_long() {
        run_line_test("abcdefghijklmnopqrstuvwxyz abcdefghijklmnopqrstuvwxyz");
    }

    #[test]
    fn simple_read_utf8() {
        run_line_test("😀 😃 😄 😁 😆 😅 😂 🤣 🥲 🥹");
    }
}