use std::{net::{TcpStream, SocketAddr}, collections::HashMap, rc::Rc};

#[derive(Copy, Clone, Debug, PartialEq)]
enum State {
    Joined,
    LobbySelection,
    InvalidInput
}

pub struct GameRoom {
    pub name : String
}

pub struct User {
    pub prev_prompt : String,
    state : State,
    prev_state : State,
    pub game_room : Option<Rc<GameRoom>>
}

pub struct GameServerState {
    pub user_state : HashMap<SocketAddr, User>,
    pub game_rooms : Vec<Rc<GameRoom>>
}

impl GameServerState {
    fn get_lobby_listing(self : &GameServerState) -> String {
        let rooms = &self.game_rooms;
        let mut out = "0: New Lobby\r\n".to_string();
        for (i, room) in rooms.iter().enumerate() {
            out.push_str(&format!("{}: {:>15}\r\n", i+1, room.name));
        }
        out
    }

    pub fn get_client_prompt(self : &mut GameServerState, stream : &mut TcpStream) -> String {
        let user_state = get_user_state(&mut self.user_state, stream);
        match user_state.state {
            State::Joined => {
                "Connected to Telnet Codenames\r\n".to_string()
            },
            State::LobbySelection => {
                "Which lobby do you want to join? Or create a new lobby\r\n".to_string() +
                    &self.get_lobby_listing()
            },
            State::InvalidInput => {
                "Invalid input, please try again\r\n".to_string()
            }
        }
    }
    
    pub fn client_logic(self : &mut GameServerState, stream : &mut TcpStream, line : Option<String>) {
        let user_state = get_user_state(&mut self.user_state, stream);
        let game_rooms = &mut self.game_rooms;
        let starting_state = user_state.state;
        match user_state.state {
            State::Joined => {
                user_state.state = State::LobbySelection;
            },
            State::LobbySelection => lobby_selection_logic(user_state, game_rooms, &line),
            State::InvalidInput => {
                // go back to the last state
                user_state.state = user_state.prev_state;
            }
        }
        // keep track of previous states
        if user_state.state != starting_state {
            user_state.prev_state = starting_state;
        }
    }

    pub fn client_disconnect(self : &mut GameServerState, stream : &mut TcpStream) {
        // do any disconnect actions
        let _ = super::write(stream, "Goodbye");
        // remove user state from being tracked
        self.user_state.remove(&stream.peer_addr().unwrap());
    }

    pub fn new() -> GameServerState {
        GameServerState { user_state: HashMap::new(), game_rooms: Vec::new() }
    }
}

pub fn get_user_state<'a>(user_state : &'a mut HashMap<SocketAddr,User>, stream : &TcpStream) -> &'a mut User {
    let peer_addr = stream.peer_addr().unwrap();
    user_state.entry(peer_addr).or_insert(User { 
        prev_prompt: "".to_owned(), 
        game_room: None,
        state: State::Joined,
        prev_state: State::Joined
    })
}

fn lobby_selection_logic(user_state : &mut User, game_rooms : &mut Vec<Rc<GameRoom>>, line : &Option<String>) {
    // only process if there's input
    if line.is_none() {
        return;
    }
    match line.clone().unwrap().trim().parse::<usize>() {
        Ok(mut room_idx) => {
            // if this lobby index is valid (within range, or 0 to create a new one)
            // then go into that lobby
            if room_idx == 0 {
                let room = Rc::new(GameRoom { name: "New Room".to_string() });
                game_rooms.push(room);
                room_idx = game_rooms.len();
            } 
            let room_option = game_rooms.get(room_idx - 1);
            if room_option.is_none() {
                user_state.state = State::InvalidInput;
                return;
            }
            user_state.game_room = Some(room_option.unwrap().clone());
            // TODO: transition to next state
        },
        Err(_) => {
            user_state.state = State::InvalidInput;
        }
    };
}