use std::{net::{TcpStream, SocketAddr}, collections::HashMap};
use std::cmp::max;

use crate::codenames::{codenames_logic, CodenamesRoom, CodenamesPlayer, codenames_prompt, codenames_disconnect};

// State of the user in the server
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ServerState {
    Joined, // Initial state, first joined server
    UsernameEntry, // Enter Username
    LobbySelection, // Selecting lobby
    InvalidInput, // Any time invalid input is inserted
    InRoom, // In game room
    FatalError
}

pub struct GameRoom {
    pub name : String,
    pub impl_room : Option<CodenamesRoom>
}

#[derive(Clone)]
pub struct User {
    pub prev_prompt : String,
    pub state : ServerState,
    prev_state : ServerState,
    pub user_name : String,
    pub socket_addr : SocketAddr,
    pub game_room_key : Option<i32>,
    pub player : Option<CodenamesPlayer>
}

pub struct GameServerState {
    pub user_state : HashMap<SocketAddr, User>,
    pub game_rooms : HashMap<i32, GameRoom>
}

pub struct GameError {

}

impl GameServerState {
    fn get_lobby_listing(&self) -> String {
        let rooms = &self.game_rooms;
        let mut out = "0: New Lobby\r\n".to_string();
        let mut vals : Vec<(i32, &GameRoom)> = rooms.iter().map(|x| (*x.0, x.1)).collect();
        vals.sort_by(|a, b| a.0.cmp(&b.0));
        for room_val in vals {
            out.push_str(&format!("{}: {:>15}\r\n", room_val.0, room_val.1.name));
        }
        out
    }

    pub fn get_client_prompt(&mut self, stream : &mut TcpStream) -> Option<String> {
        let user_state_map = &mut self.user_state;
        let user_state = get_user_state(user_state_map, stream);
        match user_state.state {
            ServerState::Joined => {
                Some("Connected to Telnet Codenames\r\n".to_string())
            },
            ServerState::UsernameEntry =>
                Some("Enter in your username, maximum of 25 characters\r\n".to_string()),
            ServerState::LobbySelection => {
                Some("Which lobby do you want to join? Or create a new lobby\r\n".to_string() +
                    &self.get_lobby_listing())
            },
            ServerState::InvalidInput => {
                Some("Invalid input, please try again\r\n".to_string())
            },
            ServerState::InRoom => codenames_prompt(stream,
                                                    user_state_map,
                                                    &mut self.game_rooms),
            ServerState::FatalError => {
                Some("A fatal error has occurred, disconnecting...\r\n".to_string())
            }
        }
    }
    
    pub fn client_logic(&mut self, stream : &mut TcpStream, line : Option<String>) -> Result<(), GameError> {
        let user_state_map = &mut self.user_state;
        let user_state = get_user_state(user_state_map, stream);
        let game_rooms = &mut self.game_rooms;
        let starting_state = user_state.state;
        match user_state.state {
            ServerState::Joined => {
                user_state.state = ServerState::UsernameEntry;
            },
            ServerState::UsernameEntry => {
                // TODO: factor out into username entry logic?
                if line.is_some() {
                    match line.filter(|l| l.len() <= 25) {
                        Some(l) => {
                            user_state.user_name = l.trim().to_string();
                            user_state.state = ServerState::LobbySelection;
                        },
                        None => {
                            user_state.state = ServerState::InvalidInput;
                        }
                    }
                }
            },
            ServerState::LobbySelection => lobby_selection_logic(user_state, game_rooms, &line),
            ServerState::InvalidInput => {
                // go back to the last state
                user_state.state = user_state.prev_state;
            },
            ServerState::FatalError => {
                return Err(GameError {  });
            }
            ServerState::InRoom => codenames_logic(stream,
                                                   user_state_map,
                                                   &mut self.game_rooms,
                                                   &line)
        }
        // keep track of previous states
        let user_state = get_user_state(user_state_map, stream);
        if user_state.state != starting_state {
            user_state.prev_state = starting_state;
        }
        Ok(())
    }

    pub fn client_disconnect(&mut self, stream : &mut TcpStream) {
        // do any disconnect actions
        let _ = super::write(stream, "Goodbye\r\n");
        let addr = stream.peer_addr().unwrap();
        codenames_disconnect(addr, &mut self.game_rooms, &mut self.user_state);
        // remove user state from being tracked
        self.user_state.remove(&addr);
    }

    pub fn new() -> GameServerState {
        GameServerState { user_state: HashMap::new(), game_rooms: HashMap::new() }
    }
}

pub fn get_user_state<'a>(user_state : &'a mut HashMap<SocketAddr,User>, stream : &TcpStream) -> &'a mut User {
    let peer_addr = stream.peer_addr().unwrap();
    user_state.entry(peer_addr).or_insert(User { 
        prev_prompt: "".to_owned(), 
        game_room_key: None,
        state: ServerState::Joined,
        prev_state: ServerState::Joined,
        socket_addr: peer_addr,
        player: None,
        user_name: "default".to_string()
    })
}

/// Finds an empty slot in the game room hash map and returns that index
/// this can/should be turned into a more efficient implementation
/// that uses vectors and indices
fn find_empty_slot(game_rooms : &HashMap<i32, GameRoom>) -> i32 {
    // TODO: replace this implementation with a proper slot map
    let mut last_idx = 0;
    for room_iter in game_rooms.iter() {
        last_idx = max(last_idx, *room_iter.0);
    }
    last_idx + 1
}

fn lobby_selection_logic(user_state : &mut User, game_rooms : &mut HashMap<i32, GameRoom>, line : &Option<String>) {
    // only process if there's input
    if line.is_none() {
        return;
    }
    match line.clone().unwrap().trim().parse::<i32>() {
        Ok(mut room_idx) => {
            // if this lobby index is valid (within range, or 0 to create a new one)
            // then go into that lobby
            if room_idx == 0 { // create new lobby
                let room = GameRoom { name: user_state.user_name.to_string() + "'s Room", impl_room: None };
                room_idx = find_empty_slot(game_rooms);
                game_rooms.insert(room_idx, room);
            } 
            let room_option = game_rooms.get(&room_idx);
            if room_option.is_none() {
                user_state.state = ServerState::InvalidInput;
                return;
            }
            user_state.game_room_key = Some(room_idx);
            user_state.state = ServerState::InRoom;
        },
        Err(_) => {
            user_state.state = ServerState::InvalidInput;
        }
    };
}