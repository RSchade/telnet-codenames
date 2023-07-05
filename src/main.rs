use std::{net::TcpListener, io::Result};

fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:1234")?;
    return telnet_codenames::event_loop(listener);
}

