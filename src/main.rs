extern crate twilio;
extern crate rand;
extern crate hyper;
extern crate rustc_serialize;

use std::collections::HashSet;
use std::thread;
use std::sync::{Arc, Mutex};
use rustc_serialize::json;
use rand::Rng;
use std::env;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use twilio::{Client, OutboundMessage};
use std::io::Read;


// trait Player {
//     fn send_msg(&self, String);
//     fn receive_msg(&self);
// }

type GameMsg = TxtMessage;
type PlayerId = String;

#[derive(Debug, Clone)]
struct Player {
    player_id: PlayerId,
}

type BusyPlayers = Arc<Mutex<HashSet<PlayerId>>>;

trait PlayerMsg {
    fn send_msg(&self, msg: &str);
    fn send_msg_to_other_player(&self, other: &Player, msg: &str);
}

struct TwilioPlayerWithMessage {
    player: Player,
    from: String,
    client: twilio::Client,
}

impl PlayerMsg for TwilioPlayerWithMessage {
    fn send_msg(&self, msg: &str) {
        self.send_msg_to_other_player(&self.player, msg);
    }

    fn send_msg_to_other_player(&self, other: &Player, msg: &str) {
        println!("Sending txt message to {:?} about {:?}", self.player.player_id, msg);
        self.client.send_message(OutboundMessage::new(&self.from, &other.player_id, msg)).unwrap();
    }
}

type GameId = String;

struct Game {
    main_player: Player,
    friend: Player,
    stranger: Player,
    receiver: Receiver<TxtMessage>,
}

trait MessagingLayer {
    // Block until game is ready
    fn start(&self, game_id: GameId, main_player: Player, stranger: Player, sender: Receiver<TxtMessage>, busy_players: BusyPlayers) -> Game;
    fn new_game_id(&self) -> GameId;
}

#[derive(Clone)]
struct TwilioCreds {
    sid: String,
    auth: String,
    from: String,
}

#[derive(Clone)]
struct TwilioLayer {
    creds: TwilioCreds,
}

impl MessagingLayer for TwilioLayer {
    fn start(&self, game_id: GameId, main_player: Player, stranger: Player, receiver: Receiver<TxtMessage>, busy_players: BusyPlayers) -> Game {

        let mut friend: Option<Player> = None;

        let twilio_client: twilio::Client = Client::new(&self.creds.sid, &self.creds.auth);

        let main_player: TwilioPlayerWithMessage = TwilioPlayerWithMessage{
            player: main_player,
            client: twilio_client,
            from: self.creds.from.clone(),
        };

        main_player.send_msg(&format!("This is your game code: {}", game_id));
        main_player.send_msg(&format!("Have a friend join by texting: join {}", game_id));

        loop {
            let m = receiver.recv().unwrap();
            if m.body.to_lowercase() == format!("join {}", game_id) && !busy_players.lock().unwrap().contains(&m.from) {
                let p = Player{player_id: m.from};
                main_player.send_msg_to_other_player(&p, "[ref] Welcome friend!");
                main_player.send_msg(&format!("[ref] Your friend at {} has joined!\nRemember type: \"stranger danger\" if you think this is a stranger\nand type: \"buddy buddy\" if this is your friend.\nStart Chatting!", p.player_id.clone()));
                friend = Some(p);
            }

            match &friend {
                &Some(_) => { break; },
                _ => {},
            }
        }

        let friend = friend.unwrap();

        return Game{
            main_player: main_player.player,
            friend: friend,
            stranger: stranger,
            receiver: receiver
        }
    }

    fn new_game_id(&self) -> GameId {
        let game_ids = vec![ "spare", "poised", "measure", "impartial", "secretive", "baby", "scintillating", "light", "start", "dear", "vessel", "men", "tall", "reproduce", "tranquil", "alcoholic", "rinse", "airplane", "name", "harmony"];

        let mut s = String::new();
        s.push_str(rand::thread_rng().choose(&game_ids).unwrap());
        return s;
    }
}

#[derive(RustcDecodable, RustcEncodable)]
pub struct TMessagesResponse  {
	messages: Vec<TxtMessage>,
}

#[derive(RustcDecodable, RustcEncodable, Debug, Clone)]
struct TxtMessage {
	from: String,
	body: String,
	date_created: String,
    sid: String,
}

type MsgListeners = Arc<Mutex<Vec<Sender<TxtMessage>>>>;

struct TwilioListener {
    listeners: MsgListeners,
    twilio_creds: TwilioCreds,
}

impl TwilioListener {
    fn start_polling(&self) {
        let mut seen_messages: HashSet<String> = HashSet::new();
        for m in get_messages(&self.twilio_creds).into_iter() {
            seen_messages.insert(m.sid.clone());
        }

        loop {
            let messages = get_messages(&self.twilio_creds);
            let unique_messages = messages.into_iter().filter(|m| !seen_messages.contains(&m.sid.clone())).collect::<Vec<TxtMessage>>();

            for m in unique_messages.into_iter() {
                println!("sending a message: {:?}", m);
                seen_messages.insert(m.sid.clone());
                for s in self.listeners.lock().unwrap().iter() {
                    match s.send(m.clone()) {
                        Ok(()) => {},
                        Err(e) => { println!("Error on pushing message {:?}: {:?}", m, e) },
                    }
                }
            }

            std::thread::sleep(std::time::Duration::new(1, 0));
        }

    }
}

fn get_messages(twilio_creds: &TwilioCreds) -> Vec<TxtMessage> {
    let auth = hyper::header::Basic{
        username: twilio_creds.sid.clone(),
        password: Some(twilio_creds.auth.clone()),
    };

    let client = hyper::Client::new();

    // Creating an outgoing request.
    let mut res = client.get(&format!("https://api.twilio.com/2010-04-01/Accounts/{AccountSid}/Messages.json?To={PhoneNumber}", AccountSid=&twilio_creds.sid, PhoneNumber=&twilio_creds.from))
        // set a header
        .header(hyper::header::Authorization(auth))
        // let 'er go!
        .send().unwrap();

    // Read the Response.
    let mut body = String::new();
    res.read_to_string(&mut body).unwrap();
    let decoded: TMessagesResponse = json::decode(&body).unwrap();
    return decoded.messages;
}

#[derive(Copy, Clone, Debug)]
enum CoinFlip {
    Stranger,
    Friend,
}

fn coin_flip() -> CoinFlip {
    let x = rand::random::<u8>();
    if x%2 == 0 {
        return CoinFlip::Stranger;
    }
    return CoinFlip::Friend;
}

fn start_game(main_player: Player, stranger: Player, twilio_layer: TwilioLayer, listeners: MsgListeners, busy_players: BusyPlayers) {
    // Then build a game
    //  -> which will wait for users to register (3 people)
    let game_id = twilio_layer.new_game_id();
    let (tx, rx): (Sender<TxtMessage>, Receiver<TxtMessage>) = mpsc::channel();

    listeners.lock().unwrap().push(tx);

    let game = twilio_layer.start(game_id, main_player, stranger, rx, busy_players.clone());

    let stranger_clone = game.stranger.clone();
    let friend_clone = game.friend.clone();

    let partner: Player;
    let flip_result = coin_flip();
    match flip_result{
        CoinFlip::Stranger => partner = game.stranger,
        CoinFlip::Friend => partner = game.friend,
    }

    let twilio_client: twilio::Client = Client::new(&twilio_layer.creds.sid, &twilio_layer.creds.auth);
    let twilio_client2: twilio::Client = Client::new(&twilio_layer.creds.sid, &twilio_layer.creds.auth);

    let partner: TwilioPlayerWithMessage = TwilioPlayerWithMessage{
        player: partner,
        client: twilio_client,
        from: twilio_layer.creds.from.clone(),
    };

    let main_player: TwilioPlayerWithMessage = TwilioPlayerWithMessage{
        player: game.main_player,
        client: twilio_client2,
        from: twilio_layer.creds.from.clone(),
    };

    println!("Partner is: {:?}", partner.player);
    main_player.send_msg("[Ref]: We've matched you up and you're ready to go!");
    main_player.send_msg("[Ref]: You win if you guess if you're talking to a friend or a stranger correctly!");
    partner.send_msg("[Ref]: We've matched you up and you're ready to go!");

    match flip_result {
        CoinFlip::Friend => {
            // Remove the stranger from busy players
            busy_players.lock().unwrap().remove(&stranger_clone.player_id);
            // Add our friend to busy players
            busy_players.lock().unwrap().insert(partner.player.player_id.clone());

            partner.send_msg("[Ref]: The game has started!");
            partner.send_msg("[Ref]: You win this game if you convince your friend that you are a stranger!\nGame On!!");
        },
        CoinFlip::Stranger => {
            partner.send_msg_to_other_player(&friend_clone, "[Ref]: We've matched your friend up to a stranger. Don't spoil it!");
            partner.send_msg_to_other_player(&friend_clone, "[Ref]: You are out of this game now, but you can join or start another game");
            partner.send_msg("[Ref]: The game has started!");
            partner.send_msg("[Ref]: You win this game if you convince the other person you are their friend\nGame On!!");

        }
    }

    loop {
        let m = game.receiver.recv().unwrap();

        if m.from != main_player.player.player_id && m.from != partner.player.player_id {
            continue;
        }

        if m.from == partner.player.player_id {
            main_player.send_msg(&m.body);
        } else if m.from == main_player.player.player_id {
            partner.send_msg(&m.body);
        }

        if m.from == main_player.player.player_id {
            match flip_result {
                CoinFlip::Stranger => {
                    if m.body.to_lowercase() == "stranger danger" {
                        main_player.send_msg("[Ref here]: you got it! You win! \\o/");
                        partner.send_msg("[Ref here]: They guessed right, you've lost! :(");
                        finish_game_clean_busy_players(&main_player.player, &partner.player, busy_players);
                        break;
                    } else if m.body.to_lowercase() == "buddy buddy" {
                        main_player.send_msg("[Ref here]: sorry dude, you got it wrong! You Lose! :(");
                        partner.send_msg("[Ref here]: Nice work, you fooled them! :)");
                        finish_game_clean_busy_players(&main_player.player, &partner.player, busy_players);
                        break;
                    }
                },
                CoinFlip::Friend => {
                    if m.body.to_lowercase() == "stranger danger" {
                        main_player.send_msg("[Ref here]: sorry dude, you got it wrong! You Lose! :(");
                        partner.send_msg("[Ref here]: Nice work, you fooled them! :)");
                        finish_game_clean_busy_players(&main_player.player, &partner.player, busy_players);
                        break;
                    } else if m.body.to_lowercase() == "buddy buddy" {
                        main_player.send_msg("[Ref here]: you got it! You win! \\o/");
                        partner.send_msg("[Ref here]: They guessed right, you've lost! :(");
                        finish_game_clean_busy_players(&main_player.player, &partner.player, busy_players);
                        break;
                    }
                },
            }
        }

        if m.body.to_lowercase() == "game over" {
            main_player.send_msg("[Ref here]: Game over");
            partner.send_msg("[Ref here]: Game over");
            println!("Quitting game");
            finish_game_clean_busy_players(&main_player.player, &partner.player, busy_players);
            break;
        }

    }

    // TODO send back receiver

}

fn finish_game_clean_busy_players(main_player: &Player, partner: &Player, busy_players: BusyPlayers) {
    println!("Removing busy players {:?} and {:?} since game is over", main_player.player_id, partner.player_id);
    busy_players.lock().unwrap().remove(&main_player.player_id);
    busy_players.lock().unwrap().remove(&partner.player_id);
}

fn play_game (twilio_layer: TwilioLayer, listeners: MsgListeners, stranger_receiver: Receiver<Stranger>, busy_players: BusyPlayers) {

    // First listen for a start game request: saying "who dis"
    let (start_tx, start_rx): (Sender<TxtMessage>, Receiver<TxtMessage>) = mpsc::channel();
    listeners.lock().unwrap().push(start_tx);

    loop {
        let m_result = start_rx.recv();
        match m_result {
            Ok(ref m) => {
                println!("Received message: {:?}", m);
                if m.body.to_lowercase() == "who dis" && !busy_players.lock().unwrap().contains(&m.from) {
                    println!("Adding {:?} to busy players:", m.from);
                    busy_players.lock().unwrap().insert(m.from.clone());
                    let main_player = Player{player_id: m.from.clone()};
                    let twilio_client: twilio::Client = Client::new(&twilio_layer.creds.sid, &twilio_layer.creds.auth);

                    let main_player: TwilioPlayerWithMessage = TwilioPlayerWithMessage{
                        player: main_player,
                        client: twilio_client,
                        from: twilio_layer.creds.from.clone(),
                    };

                    main_player.send_msg("Waiting for stranger to join");
                    let stranger = stranger_receiver.recv().unwrap();

                    println!("Adding stranger {:?} to busy players:", stranger.player_id);
                    busy_players.lock().unwrap().insert(stranger.player_id.clone());

                    main_player.send_msg("Found a stranger! now time to invite your friend");

                    let lc = listeners.clone();
                    let tl = twilio_layer.clone();
                    let bp = busy_players.clone();

                    thread::spawn(move || {
                        println!("Starting new thread for game");
                        start_game(main_player.player, stranger, tl, lc, bp);
                        println!("Finished with game, killing thread");
                    });
                }
            },
            Err(e) => {
                println!("Something failed when receiving start game: {:?}", e);
            }
        }
    }

    // after we have channels for each person
    // decide if we are doing friend or stranger
    // hook up two people. Wait on messages
    // If person says "stranger danger" guess stranger
    // If person says "buddy buddy" guess friend
    //
    // 1 thread per game
}

type Stranger = Player;

fn setup_stranger_listener (listeners: MsgListeners, stranger_sender: Sender<Stranger>, busy_players: BusyPlayers) {
    let (sender, receiver): (Sender<TxtMessage>, Receiver<TxtMessage>) = mpsc::channel();
    listeners.lock().unwrap().push(sender);

    loop {
        match receiver.recv() {
            Ok(ref m) => {
                if m.body.to_lowercase() == "stranger join" && !busy_players.lock().unwrap().contains(&m.from) {
                    match stranger_sender.send(Player{player_id: m.from.clone()}) {
                        Ok(_) => {},
                        Err(e) => {
                            println!("Error on sending stranger joins, {:?}", e);
                        }
                    }
                }
            },
            Err(e) => {
                println!("Error on reading stranger joins, {:?}", e);
            }
        }
    }
}

fn main() {
    println!("Hello, world!");

    let twilio_creds = TwilioCreds{
        sid: env::var("TWILIO_SID").unwrap(),
        auth: env::var("TWILIO_AUTH").unwrap(),
        from: env::var("TWILIO_FROM").unwrap(),
    };

    let busy_players: BusyPlayers = Arc::new(Mutex::new(HashSet::new()));

    let listeners = Arc::new(Mutex::new(Vec::new()));
    let listener = TwilioListener{listeners: listeners.clone(), twilio_creds: twilio_creds.clone()};

    let twilio_layer = TwilioLayer{creds: twilio_creds.clone()};

    let (stranger_sender, stranger_receiver): (Sender<Stranger>, Receiver<Stranger>) = mpsc::channel();

    thread::spawn(move || {
        listener.start_polling();
    });

    let stranger_listeners_clone = listeners.clone();
    let busy_players_clone = busy_players.clone();

    thread::spawn(move || {
        setup_stranger_listener(stranger_listeners_clone, stranger_sender, busy_players_clone)
    });

    println!("Starting game!");
    play_game(twilio_layer, listeners, stranger_receiver, busy_players);
}
