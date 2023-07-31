use std::collections::HashMap;
use crate::game::{GameRoom, User, ServerState};

// State of the Codenames game room
#[derive(Copy, Clone, Debug, PartialEq)]
enum CodenamesState {
    WaitingToStart,
    RedTurn,
    BlueTurn,
    GameEnd
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum CodenamesTeam {
    Red,
    Blue
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum CodenamesRole {
    Spymaster,
    Teammate
}

pub struct CodenamesPlayer {
    team : Option<CodenamesTeam>,
    role : Option<CodenamesRole>
}

pub struct CodenamesRoom<'a> {
    state : CodenamesState,
    players : Vec<&'a User>
}

/// Initializes the board if necessary for the Codenames game
/// returns the relevant Codenames room
fn initialize_user_board<'a>(user_state : &mut User, game_rooms: &'a mut HashMap<i32, GameRoom>) -> Result<&'a CodenamesRoom<'a>, ()>{
    // create room if not already there
    // put the user and the room in the beginning states
    user_state.player.get_or_insert(CodenamesPlayer {
        team: None,
        role: None,
    });
    match user_state.game_room_key {
        Some(room) => {
            match game_rooms.get_mut(&room) {
                Some(room) => {
                    if room.impl_room.is_none() {
                        //let mut players = Vec::new();
                        //players.push(&*user_state);
                        room.impl_room = Some(CodenamesRoom {
                            state: CodenamesState::WaitingToStart,
                            players: Vec::new()
                        });
                    }
                    Ok(room.impl_room.as_ref().unwrap())
                },
                None => {
                    // there should always be an existing room when
                    // running this function
                    user_state.state = ServerState::FatalError;
                    Err(())
                }
            }
        },
        None => {
            // there should always be an existing room when
            // running this function
            user_state.state = ServerState::FatalError;
            Err(())
        }
    }
}

impl CodenamesRoom<'_> {
    /// Returns a string representing a board's state for a given
    /// team and role type
    fn get_board(&self, team : CodenamesTeam, role : CodenamesRole) -> String {
        "".to_string()
    }

    /// Shows the roles of all the room's players
    fn get_player_roles(&self) -> String {
        "".to_string()
    }
}

/// Prompt generation function for a given user
pub fn codenames_prompt(user_state : &mut User, game_rooms : &mut HashMap<i32, GameRoom>) -> Option<String> {
    match initialize_user_board(user_state, game_rooms) {
        Ok(room) => {
            match room.state {
                CodenamesState::WaitingToStart => {
                    Some("Input 0 or 1 to put yourself into the correct role\r\n".to_string() +
                         &"Input 'start' to start the game if the correct roles are filled\r\n".to_string() +
                         &room.get_player_roles())
                },
                CodenamesState::BlueTurn => {Some("Blue Turn".to_string())},
                CodenamesState::RedTurn => {Some("Red Turn".to_string())},
                CodenamesState::GameEnd => {Some("End".to_string())}
            }
        },
        Err(_) => {None} // TODO: should do something here
    }
}

/// Processes the input from a user
pub fn codenames_logic(user_state : &mut User, game_rooms : &mut HashMap<i32, GameRoom>, line : &Option<String>) {
    // based on the state of the room and the user
    // allow specific actions
    match initialize_user_board(user_state, game_rooms) {
        Ok(room) => {
            // TODO: is it possible for this unwrap to panic?
            let player = user_state.player.as_mut().unwrap();
            match room.state {
                CodenamesState::WaitingToStart => {
                    line.clone().map(|line| match line.as_str().trim() {
                        "start" => {
                            // verify conditions are correct, then start the game
                            // tell the room which player started the game?
                        },
                        "0" => {
                            player.role = Some(CodenamesRole::Teammate);
                        },
                        "1" => {
                            player.role = Some(CodenamesRole::Spymaster);
                        },
                        _ => {
                            user_state.state = ServerState::InvalidInput;
                        }
                    });
                },
                CodenamesState::BlueTurn => {},
                CodenamesState::RedTurn => {},
                CodenamesState::GameEnd => {}
            }
        },
        Err(_) => {}
    }
}