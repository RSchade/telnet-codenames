use std::collections::{HashMap, HashSet, VecDeque};
use std::{fmt, fs};
use std::net::{SocketAddr, TcpStream};
use rand::thread_rng;
use rand::prelude::IteratorRandom;
use crate::game::{GameRoom, User, ServerState, get_user_state};

// State of the Codenames game room
#[derive(Copy, Clone, Debug, PartialEq)]
enum CodenamesState {
    WaitingToStart,
    RedTurn,
    BlueTurn,
    GameEnd
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum CodenamesTeam {
    Red,
    Blue,
    Floating
}

impl fmt::Display for CodenamesTeam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CodenamesTeam::Red => write!(f, "{}", "Red"),
            CodenamesTeam::Blue => write!(f, "{}", "Blue"),
            CodenamesTeam::Floating => write!(f, "{}", "Floating")
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum CodenamesRole {
    Spymaster,
    Teammate,
    Spectator
}

#[derive(Clone)]
pub struct CodenamesPlayer {
    team : CodenamesTeam,
    role : CodenamesRole,
    chat_queue : VecDeque<String>,
    state_prompted : Option<CodenamesState> // last state prompted
}

impl Default for CodenamesPlayer {
    fn default() -> Self {
        Self {
            team: CodenamesTeam::Floating,
            role: CodenamesRole::Spectator,
            chat_queue: VecDeque::new(),
            state_prompted: None
        }
    }
}

impl Default for &CodenamesPlayer {
    fn default() -> Self {
        static PLAYER: CodenamesPlayer = CodenamesPlayer {
            team: CodenamesTeam::Floating,
            role: CodenamesRole::Spectator,
            chat_queue: VecDeque::new(),
            state_prompted: None
        };
        &PLAYER
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CodenamesCardType {
    RedAgent,
    BlueAgent,
    Assassin,
    Bystander
}

impl fmt::Display for CodenamesCardType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CodenamesCardType::RedAgent => write!(f, "R"),
            CodenamesCardType::BlueAgent => write!(f, "B"),
            CodenamesCardType::Assassin => write!(f, "A"),
            CodenamesCardType::Bystander => write!(f, " ")
        }
    }
}

struct CodenamesCard {
    word : String,
    card_type : CodenamesCardType,
    flipped : bool
}

struct CodenamesClue {
    cards_to_match : i32,
    clue: String
}

pub struct CodenamesRoom {
    state : CodenamesState,
    players : HashSet<SocketAddr>,
    red_score : i32,
    blue_score : i32,
    guesses : i32,
    assassin_found_by : Option<CodenamesTeam>,
    clue: Option<CodenamesClue>,
    board : [[CodenamesCard; 5]; 5]
}

fn gen_board() -> [[CodenamesCard; 5]; 5] {
    let word_list = String::from_utf8_lossy(
        include_bytes!("./wordlist-eng.txt"))
        .to_string();
    // Get a complete list of all the words used for the game
    let mut words : Vec<&str> = word_list.split('\n')
        .map(|w| w.trim())
        .collect();
    // Get a list of all the card types used to pick from
    // 8 blue agent, 9 red agent, 7 bystanders, 1 assassin
    let mut card_types : Vec<&CodenamesCardType> =
        [CodenamesCardType::BlueAgent].iter()
            .cycle().take(8).chain(
        [CodenamesCardType::RedAgent].iter()
            .cycle().take(9)).chain(
        [CodenamesCardType::Bystander].iter()
            .cycle().take(7)).chain(
        [CodenamesCardType::Assassin].iter())
            .collect();
    if card_types.len() != 25 {
        panic!("Word length doesn't equal the card type length");
    }
    [[(); 5]; 5].map(| x | x.map(| x | {
        // TODO: should this be a function?
        let (i, &word) = words.iter()
            .enumerate()
            .choose(&mut thread_rng())
            .unwrap();
        words.remove(i);
        let (i, &card_type) = card_types.iter()
            .enumerate()
            .choose(&mut thread_rng())
            .unwrap();
        card_types.remove(i);
        CodenamesCard {
            word: word.to_string(),
            card_type: *card_type,
            flipped: false
        }
    }))
}

/// Initializes the board if necessary for the Codenames game
/// returns the relevant Codenames room
fn initialize_user_board<'a>(user_state : &mut User, game_rooms: &'a mut HashMap<i32, GameRoom>) -> Result<&'a mut CodenamesRoom, ()>{
    // create room if not already there
    // put the user and the room in the beginning states
    user_state.player.get_or_insert(CodenamesPlayer {
        team: CodenamesTeam::Floating,
        role: CodenamesRole::Spectator,
        chat_queue: VecDeque::new(),
        state_prompted: None
    });
    match user_state.game_room_key {
        Some(room) => {
            match game_rooms.get_mut(&room) {
                // TODO: lots of unwraps here
                Some(room) => {
                    if room.impl_room.is_none() {
                        let mut players = HashSet::new();
                        players.insert(user_state.socket_addr);
                        room.impl_room = Some(CodenamesRoom {
                            state: CodenamesState::WaitingToStart,
                            players,
                            blue_score: 0,
                            red_score: 0,
                            clue: None,
                            guesses: 0,
                            assassin_found_by: None,
                            board: gen_board()
                        });
                    } else {
                        // Insert key into the players list to
                        // ensure that it's there
                        room.impl_room
                            .as_mut()
                            .unwrap()
                            .players
                            .insert(user_state.socket_addr);
                    }
                    Ok(room.impl_room.as_mut().unwrap())
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

impl CodenamesRoom {
    /// Returns a string representing a board's state for a given
    /// team and role type
    fn get_board(&self, team : CodenamesTeam, role : CodenamesRole) -> String {
        let board = &self.board;
        let mut board_str = format!("{:-<81}\r\n", "").to_string();
        for row in board {
            for card in row {
                let flipped = if card.flipped { "X".to_string() }  else { " ".to_string() };
                // If you are a teammate and the card isn't flipped over then ONLY show the word
                // otherwise you are a spymaster or the card is flipped over, show everything
                if CodenamesRole::Teammate == role && !card.flipped {
                    board_str += &format!("|{:>1}{:^14}", flipped, card.word);
                } else {
                    board_str += &format!("|{:>1}{:^13}{:>1} ", flipped, card.word, card.card_type); // TODO: flipped isn't working right
                }
            }
            board_str += &format!("|\r\n{:-<81}\r\n", "");
        }
        board_str
    }
}

/// Shows the roles of all the room's players
fn get_player_roles(room : &CodenamesRoom, user_state_map : &HashMap<SocketAddr, User>, cur_user_addr : SocketAddr) -> String {
    let list_str : String = room.players.iter().map(|room_player_addr|
        user_state_map.get(&room_player_addr)
            .map_or("".to_string(), |u| format!("{:>3} {:>25} {:>10?}, {:>10?}\r\n",
                                                if cur_user_addr == u.socket_addr { "YOU" } else { "" },
                                                &u.user_name,
                                                &u.player.as_ref().unwrap_or_default().role,
                                                &u.player.as_ref().unwrap_or_default().team)))
            .collect();
    format!("{:>29} {:>9} {:>9}\r\n{:-<49}\r\n", "User Name", "Role", "Team", "") +
        list_str.as_str() +
        &format!("{:-<49}\r\n", "")
}

fn codenames_turn_prompt(team : CodenamesTeam, player : &CodenamesPlayer, room : &CodenamesRoom) -> String {
    let mut out = format!("{} Team's Turn:\r\n", team);
    if (CodenamesRole::Spymaster, team) == (player.role, player.team) {
        out += "Type in your clue in the format 'clue,number' where a clue is a single word\
            and the number is the number of guesses your team has. Keep in mind you can't use\
            the word you would like them to choose in the guess\r\n";
    } else if  (CodenamesRole::Teammate, team) == (player.role, player.team) {
        out += "Use chat to talk to everyone but ";
        out += format!("the spymaster on the {} team. ", team).as_str();
        out += "Guess by submitting your guess word with a '!' in front. \
            End your turn with '!!' after making at least one guess.\r\n";
    } else {
        out += "Continue to talk to everyone, it's not your turn\r\n";
    }
    out += &format!("Score: {}-{} (R-B)\r\n", room.red_score, room.blue_score);
    out += room.get_board(player.team, player.role).as_str();
    return out;
}

/// Prompt generation function for a given user
pub fn codenames_prompt(user_stream : &TcpStream, user_state_map : &mut HashMap<SocketAddr, User>,
                        game_rooms : &mut HashMap<i32, GameRoom>) -> Option<String> {
    let user_state = get_user_state(user_state_map, user_stream);
    let user_addr = user_state.socket_addr;
    // total output message (including all chat messages and prompt)
    let mut prompt : Vec<String> = Vec::new();
    // process user's chat queue
    if let Some(ref mut player) = user_state.player {
        while let Some(msg) = player.chat_queue.pop_front() {
            prompt.push(msg);
        }
    }
    match initialize_user_board(user_state, game_rooms) {
        Ok(room) => {
            let player = user_state.player.as_mut().unwrap();
            if player.state_prompted.is_none() ||
                player.state_prompted.is_some_and(|state_prompted| state_prompted != room.state) {
                player.state_prompted = Some(room.state);
                match room.state {
                    CodenamesState::WaitingToStart => { // TODO: refresh for all players if this prompt changes
                        prompt.push("Available Options:\r\n".to_string() +
                            "teammate/spymaster: Put yourself in one of these roles\r\n" +
                            "red/blue: Put yourself into one of these teams\r\n" +
                            "show: Show the current state of the room if there are any changes\r\n" +
                            "start: Start the game if the correct roles are filled\r\n" +
                            "Otherwise, any other input will be a chat message to the room\r\n" +
                            &get_player_roles(&room, user_state_map, user_addr))
                    },
                    CodenamesState::BlueTurn => prompt.push(codenames_turn_prompt(CodenamesTeam::Blue, player, room)),
                    CodenamesState::RedTurn => prompt.push(codenames_turn_prompt(CodenamesTeam::Red, player, room)),
                    CodenamesState::GameEnd => { // TODO: not always triggering
                        prompt.push("The game has ended, thanks for playing!\r\n".to_string());
                        if let Some(found_by) = room.assassin_found_by {
                            prompt.push(format!("The {} team found the assassin, so they lost!", found_by));
                        } else {
                            prompt.push(format!("The final score was {}-{} (R-B)\r\n", room.red_score, room.blue_score))
                        }
                    }
                }
            } else {
                player.state_prompted = Some(room.state);
            }
        },
        Err(_) => {} // TODO: should do something here
    }
    if prompt.len() == 0 {
        return None;
    }
    Some(prompt.iter().map(|x| x.to_string() + "\r\n").collect())
}

/// Sends a chat message from user to everyone else in the room
/// players from the room are found using the user state map
fn broadcast_chat(user_addr : SocketAddr, user_name : String,
                  chat_line : String, room : &CodenamesRoom,
                  user_state_map : &mut HashMap<SocketAddr, User>) {
    // send as a chat message to everyone else
    for room_user in user_state_map.values_mut() {
        if room.players.contains(&room_user.socket_addr) && room_user.socket_addr != user_addr {
            if let Some(ref mut room_player) = room_user.player {
                room_player.chat_queue.push_back(
                    format!("{}: {}", user_name, chat_line.trim().to_string()));
            }
        }
    }
}

fn broadcast_chat_everyone(chat_line : String, room : &CodenamesRoom,
                  user_state_map : &mut HashMap<SocketAddr, User>) {
    // send as a chat message to everyone else
    for room_user in user_state_map.values_mut() {
        if room.players.contains(&room_user.socket_addr) {
            if let Some(ref mut room_player) = room_user.player {
                room_player.chat_queue.push_back(chat_line.trim().to_string());
            }
        }
    }
}

fn verify_room(room : &CodenamesRoom, user_state_map : &mut HashMap<SocketAddr, User>) -> bool {
    let mut counts = HashMap::new();
    for addr in &room.players {
        // TODO: should unwrap or is_some? it doesn't make sense if this option is None
        let user = user_state_map.get(addr).unwrap();
        if let Some(player) = &user.player {
            let k = (player.team, player.role);
            counts.entry(k).or_insert(0);
            counts.insert(k, 1 + counts[&k]);
        }
    }
    // should have at least one of the spymaster/teammate roles
    // in both red and blue
    for team in [CodenamesTeam::Red, CodenamesTeam::Blue] {
        for role in [CodenamesRole::Spymaster, CodenamesRole::Teammate] {
            let is_valid = counts.get(&(team, role)).is_some_and(|v| *v >= 1);
            if !is_valid {
                return false
            }
        }
    }
    true
}

fn refresh_prompt(room : &mut CodenamesRoom,
                  user_state_map : &mut HashMap<SocketAddr, User>) {
    for id in &room.players {
        if let Some(user) = user_state_map.get_mut(&id) {
            user.player.as_mut().unwrap().state_prompted = None;
        }
    }
}

/// Finds the card with the given card name in the codenames room, returns a mutable reference
fn find_card<'a>(card_name : &str, room : & 'a mut CodenamesRoom) -> Option<& 'a mut CodenamesCard> {
    for row in room.board.as_mut() {
        for card in row {
            if card.word == card_name {
                // found the card, output what it is
                return Some(card);
            }
        }
    }
    None
}

fn turn_logic(team : CodenamesTeam,
              line : &Option<String>,
              user_state_map : &mut HashMap<SocketAddr, User>,
              room : &mut CodenamesRoom,
              user_addr : SocketAddr,
              user_name : String) {
    let mut switch_turn = false;
    if let Some(line) = line {
        let user = user_state_map.get(&user_addr).unwrap();
        let player = user.player.as_ref().unwrap();
        if team == player.team && player.role == CodenamesRole::Teammate {
            // Teammate actions for the team
            if line.starts_with("!!") {
                // End guesses, must have guessed at least once
                if room.guesses > 0 {
                    switch_turn = true;
                } else {
                    // TODO: notify can't end
                }
            } else if line.starts_with("!") {
                // Guess
                room.guesses += 1;
                let guess = &line[1..].trim();
                broadcast_chat_everyone(format!("{} Guessed {}\r\n", user.user_name, guess),
                                        room, user_state_map);
                // check the guess, act on flipped card
                if let Some(card) = find_card(guess, room) {
                    // flip over the card so everyone can see it
                    card.flipped = true;
                    // red agents increment the red score
                    // blue agents increment the blue score
                    // bystanders switch the turn
                    // assassins end the game and cause the current team to lose
                    match card.card_type {
                        CodenamesCardType::RedAgent => {
                            room.red_score += 1;
                            if team == CodenamesTeam::Blue {
                                switch_turn = true;
                            }
                        },
                        CodenamesCardType::BlueAgent => {
                            room.blue_score += 1;
                            if team == CodenamesTeam::Red {
                                switch_turn = true;
                            }
                        },
                        CodenamesCardType::Bystander => switch_turn = true,
                        CodenamesCardType::Assassin => {
                            // end the game, this team lost
                            room.assassin_found_by = Some(team);
                            room.state = CodenamesState::GameEnd;
                            return
                        }
                    }
                    // switch turn if +1 guess than the spymaster
                    if let Some(clue) = &room.clue {
                        if room.guesses > clue.cards_to_match {
                            switch_turn = true;
                        }
                    }
                    // rebroadcast the board to everyone to take these updates into account
                    refresh_prompt(room, user_state_map);
                } else {
                    broadcast_chat_everyone(
                        format!("{} is not a valid card name to guess\r\n", guess),
                        room, user_state_map);
                }
            }
        } else if team == player.team && player.role == CodenamesRole::Spymaster {
            // Spymaster actions
            // spymaster should only say the guess word comma the number
            match line.split(',').collect::<Vec<&str>>()[..] {
                [word, number] => {
                    if let Ok(guess_number) = number.trim().parse::<i32>() {
                        room.clue = Some(CodenamesClue {
                            cards_to_match: guess_number,
                            clue: word.to_string()
                        });
                        // notify everyone of the guess
                        broadcast_chat_everyone(format!("Spymaster Clue: {}, {}\r\n",
                                                        word.to_string(), guess_number),
                                                room, user_state_map);
                    } else {
                        // TODO: notify user
                    }
                },
                _ => {
                    // TODO: notify user
                }
            }
        } else  {
            // Spectator/non participant actions
            // can talk to everyone
            // TODO: should spymasters be allowed to talk to normal players?
            broadcast_chat(user_addr, user_name,
                           line.to_string(),
                           room, user_state_map);
        }

        if switch_turn {
            room.state = if team == CodenamesTeam::Blue {
                CodenamesState::RedTurn
            } else {
                CodenamesState::BlueTurn
            };
            room.guesses = 0; // reset guesses for the new turn
            // if the end conditions are met, end the game
            // TODO: adjust when either team can go first
            if room.red_score == 9 || room.blue_score == 8 {
                room.state = CodenamesState::GameEnd;
            }
        }
    }
}

/// Processes the input from a user
pub fn codenames_logic(user_stream : &TcpStream, user_state_map : &mut HashMap<SocketAddr, User>,
                       game_rooms : &mut HashMap<i32, GameRoom>, line : &Option<String>) {
    let user_state = get_user_state(user_state_map, user_stream);
    let user_addr = user_state.socket_addr;
    let user_name = user_state.user_name.to_string();
    // Based on the state of the room, either go through the pre-game
    // initialization or the game logic itself
    match initialize_user_board(user_state, game_rooms) {
        Ok(room) => {
            // TODO: is it possible for this unwrap to panic?
            let player = user_state.player.as_mut().unwrap();
            match room.state {
                CodenamesState::WaitingToStart => {
                    if let Some(line) = line {
                        match line.trim() {
                            "start" => {
                                // verify conditions are correct, then start the game
                                // tell the room which player started the game
                                // need at least 2 players on each team,
                                // one spymaster and one teammate
                                if verify_room(room, user_state_map) {
                                    broadcast_chat_everyone(user_name.to_string() +
                                                                " Started the Game!\r\n",
                                                            room, user_state_map);
                                    room.state = CodenamesState::RedTurn;
                                } else {
                                    broadcast_chat_everyone(
                                        "Cannot start the game yet, need at least a \
                                        spymaster and a teammate on each team\r\n".to_string(),
                                            room, user_state_map);
                                }
                            },
                            "teammate" => {
                                player.role = CodenamesRole::Teammate;
                                player.state_prompted = None;
                            },
                            "spymaster" => {
                                player.role = CodenamesRole::Spymaster;
                                player.state_prompted = None;
                            },
                            "red" => {
                                player.team = CodenamesTeam::Red;
                                player.state_prompted = None;
                            },
                            "blue" => {
                                player.team = CodenamesTeam::Blue;
                                player.state_prompted = None;
                            }
                            "show" => {
                                player.state_prompted = None;
                            }
                            _ => {
                                broadcast_chat(user_addr, user_name,
                                               line.to_string(),
                                               room, user_state_map);
                            }
                        }
                    }
                },
                CodenamesState::BlueTurn => turn_logic(CodenamesTeam::Blue, line, user_state_map,
                                                       room, user_addr, user_name),
                CodenamesState::RedTurn => turn_logic(CodenamesTeam::Red, line, user_state_map,
                                                      room, user_addr, user_name),
                CodenamesState::GameEnd => {
                    // delete the room when the game ends
                    if let Some(room_key) = user_state.game_room_key {
                        game_rooms.remove(&room_key);
                    }
                }
            }
        },
        Err(_) => {}
    }
}

pub fn codenames_disconnect(addr : SocketAddr,
                            game_rooms : &mut HashMap<i32, GameRoom>,
                            user_state_map : &mut HashMap<SocketAddr, User>) {
    // remove from lobbies if in any, notify any users affected that this user has left
    // TODO: slow
    for room in game_rooms.values_mut() {
        if let Some(room) = &mut room.impl_room {
            if room.players.contains(&addr) {
                // TODO: unwrap could be wierD?
                broadcast_chat_everyone(
                    format!("{} has left the game!",
                            user_state_map.get(&addr).unwrap().user_name),
                    room, user_state_map);
                room.players.remove(&addr);
            }
        }
    }
}