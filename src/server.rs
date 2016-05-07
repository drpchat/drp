extern crate mio;
extern crate bytes;
extern crate nix;

extern crate capnp;
extern crate capnp_nonblock;

extern crate rustc_serialize;

#[macro_use]
extern crate drp;

use mio::*;
use mio::tcp::{TcpListener, TcpStream};
use mio::util::Slab;

use std::str::FromStr;
use std::collections::{HashMap, HashSet};

use rustc_serialize::hex::*;
use capnp::message::{Builder, HeapAllocator, ReaderOptions};
use capnp_nonblock::MessageStream;

use drp::util::*;
use std::io::{Write};

fn main() {
    // Create an event loop
    let mut handler = Server::new();

    let mut event_loop = EventLoop::new().unwrap();

    event_loop.register(&handler.sock, handler.token,
        EventSet::readable() | EventSet::error() | EventSet::hup(),
        PollOpt::empty()).unwrap();
    event_loop.run(&mut handler).unwrap();
}

struct Connection {
    sock: MessageStream<TcpStream>,
    token: Token,
    //interest: EventSet,

    // name: Option<String>

    // TODO this stuff is supplanted by User
    name: Option<String>,
    //pubkey: Option<Vec<u8>>,
    //channels: HashSet<String>,
}

impl Connection {
    // async write to our sock, then reregister for writable events.  the mio
    // handler will unset the writable event once our message is actually sent
    fn write_message(&mut self, event_loop: &mut EventLoop<Server>,
        msg: Builder<HeapAllocator>) {

        self.sock.write_message(msg).unwrap();
        event_loop.reregister(self.sock.inner(), self.token,
            EventSet::all(), PollOpt::empty()).unwrap();
    }
}

struct User {
    // has a lot of the stuff in connection
    //name: String, // this may be useful later, but right now is entirely redundant
    pubkey: Vec<u8>,
    channels: HashSet<String>,
    token: Token, // eventually, this will be supplanted by some general
                  // way of sending a message across the network to a user
}

struct Server {
    token: Token,
    sock: TcpListener,

    // this is going to get a hashmap of names to users, instead of connection

    channels: HashMap<String, HashSet<String>>,
    users: HashMap<String, User>,
    //names: HashMap<String, Token>,
    conns: Slab<Connection>,
}

impl Server {
    fn new() -> Server {
        Server {
            token: Token(1),
            sock: TcpListener::bind(&FromStr::from_str("0.0.0.0:8765").unwrap()).unwrap(),
            conns: Slab::new_starting_at(Token(2), 128),
            users: HashMap::new(),
            channels: HashMap::new(),
        }
    }

    // a new user is trying to register - add them to the db
    fn add_name(&mut self, token: Token, name: &str, pubkey: &[u8])
        -> Option<()> {

        if self.users.contains_key(name) {
            return None
        }

        self.users.insert(String::from(name), User {
            //name: String::from(name),
            pubkey: Vec::from(pubkey),
            channels: HashSet::new(),
            token: token,
        });
        self.conns[token].name = Some(String::from(name)); // TODO deal with nick changes

        return Some(())
    }

    // a user is joining a channel
    fn name_joins(&mut self, name: &str, channel: &str) -> Option<()> {
        //if self.channels.contains_key(name) {
        //    return None;
        //}

        // add name to channel
        let chan = self.channels.entry(String::from(channel)).or_insert(HashSet::new());
        if chan.contains(name) {
            return None
        } else {
            chan.insert(String::from(name));
        }

        // add channel to name
        if let Some(user) = self.users.get_mut(name) {
            user.channels.insert(String::from(channel));
        }

        Some(())
    }

    // a user is leaving a channel
    fn name_leaves(&mut self, name: &str, channel: &str) -> Option<()> {
        // remove name from channel
        let mut chans = match self.channels.get_mut(channel) {
            None => return None,
            Some(chans) => chans,
        };

        if !chans.remove(name) {
            return None;
        }

        // remove channel from name
        if let Some(user) = self.users.get_mut(name) {
            if !user.channels.remove(channel) {
                return None;
            }
        }

        Some(())
    }

    // name quits the server: needs to leave all channels and clear name
    fn name_quits(&mut self, name: &str) {
        for channel in self.users[name].channels.clone() {
            self.name_leaves(name, &channel);
        }

        self.users.remove(name);
    }

    fn accept(&mut self, event_loop: &mut EventLoop<Server>) {
        let sock = match self.sock.accept() {
            Ok(s) => {
                match s {
                    Some(so) => so.0,
                    None => {
                        println!("socket error of kind b");
                        return;
                    },
                }
            },
            Err(_) => {
                println!("socket error of kind a");
                return;
            },
        };

        match self.conns.insert_with(|token| Connection {
            sock: MessageStream::new(sock, ReaderOptions::default()),
            token: token,
            name: None,
        }) {

            Some(token) => {
                // register the guy
                event_loop.register(self.conns[token].sock.inner(), token,
                    EventSet::readable() | EventSet::error() | EventSet::hup(),
                    PollOpt::empty()).unwrap();
            },
            None => {
                println!("failed to make new connection");
            },
        }
    }

    fn forward(&mut self, event_loop: &mut EventLoop<Server>, token: Token) {
        // give it to the guy
        if let Some(r) = self.conns[token].sock.read_message()
            .unwrap_or_else(|e| { println!("{:?} (oh no)", e); None }) {

            match deserialize(&r).unwrap() {
                Message::Register { name, pubkey } =>
                    self.handle_register(event_loop, token, name, pubkey),
                Message::Send { dest, body, nonce } =>
                    self.handle_send(event_loop, token, dest, body, nonce),
                Message::Relay { source, dest, body, nonce } =>
                    self.handle_relay(event_loop, token, source, dest, body, nonce),
                Message::Join { channel } =>
                    self.handle_join(event_loop, token, channel),
                Message::Part { channel } =>
                    self.handle_part(event_loop, token, channel),
                Message::Response { body } =>
                    self.handle_response(event_loop, token, body),
                Message::Whois { name } =>
                    self.handle_whois(event_loop, token, name),
                Message::Theyare { name, pubkey } =>
                    self.handle_theyare(event_loop, token, name, pubkey),
            }
        } else {
            //println!("nope, let's go");
        }
    }

    fn handle_register(&mut self, event_loop: &mut EventLoop<Server>,
        token: Token, name: &str, pubkey: &[u8]) {
        eprintln!("Registering <{}> with key:\n{}", name,
            pubkey.to_hex());

        self.add_name(token, name, pubkey)
            .unwrap_or_else(|| {
            let data = serialize_response("dude ur not them");
            self.conns[token].write_message(event_loop, data);
        });
    }

    fn handle_send(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, dest: &str, body: &[u8], nonce: Option<&[u8]>) {
        let name = {
            let mut conn = &mut self.conns[token];
            match conn.name.clone() {
                Some(name) => name,
                None => {
                    let data = serialize_response("this isnt 4chan");
                    conn.write_message(event_loop, data);
                    return
                }
            }
        };

        eprintln!("sending: {} -> {}: {:?}",
            name.clone(), dest,
            String::from_utf8_lossy(body));

        if let Some(chanlist) = self.channels.get(dest) {
            for dest in chanlist {
                eprintln!("sending: {} -> {}: {:?}",
                    name.clone(),
                    dest.clone(),
                    String::from_utf8_lossy(body));

                let token = self.users.get(dest)
                    .expect("couldn't resolve dest")
                    .token;

                let data = serialize_relay(&name,
                    dest, body, nonce);

                self.conns[token].write_message(event_loop, data);
            }
        } else {
            let token = self.users.get(dest)
                .expect("couldn't resolve dest")
                .token;

            let data = serialize_relay(&name,
                dest, body, nonce);

            self.conns[token].write_message(event_loop, data);
        }
    }

    fn handle_relay(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, source: &str, dest: &str, body: &[u8],
    nonce: Option<&[u8]>) {
        eprintln!("handling relay");
        let token = self.users.get(dest).unwrap().token;
        let data = serialize_relay(source, dest, body, nonce);

        self.conns[token].write_message(event_loop, data);
    }

    fn handle_join(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, channel: &str) {
        if let Some(name) = self.conns[token].name.clone() {
            eprintln!("joining: {} -> {}",
                name.clone(),
                channel);

            self.name_joins(&name, channel).unwrap_or_else(|| {
                let data = serialize_response("you're already there!!");
                self.conns[token].write_message(event_loop, data);
            });
        } else {
            let data = serialize_response("who ARE u!?!");
            self.conns[token].write_message(event_loop, data);
        }
    }

    fn handle_part(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, channel: &str) {
        if let Some(name) = self.conns[token].name.clone() {
            eprintln!("parting: {} -> {}",
                name.clone(),
                String::from(channel));

            self.name_leaves(&name, channel).unwrap_or_else(|| {
                let data = serialize_response("you're not even there!!");
                self.conns[token].write_message(event_loop, data);
            });
        } else {
            let data = serialize_response("who ARE u!?!");
            self.conns[token].write_message(event_loop, data);
        }
    }

    fn handle_response(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, body: &str) {
        eprintln!("handle_response");

        let data = serialize_response("no ur a client");
        self.conns[token].write_message(event_loop, data);
    }

    fn handle_whois(&mut self, event_loop: &mut EventLoop<Server>,
        token: Token, name: &str) {
        eprintln!("handle_whois");

        if let Some(user) = self.users.get(name) {
            eprintln!("  got name: {}", name);
            let id = user.token;

            eprintln!("  got pubkey: {}", user.pubkey.to_hex());
            let data = serialize_theyare(name, &user.pubkey);
            self.conns[token].write_message(event_loop, data);

            // TODO this should be more explicit in the protocol
            let data = serialize_theyare(
                &self.conns[token].name.clone().unwrap(),
                &self.users[&self.conns[token].name.clone().unwrap()]
                    .pubkey.clone());
            self.conns[id].write_message(event_loop, data);

            return;
        }

        let data = serialize_response("they don't exist");
        self.conns[token].write_message(event_loop, data);
    }

    fn handle_theyare(&mut self, event_loop: &mut EventLoop<Server>,
        token: Token, name: &str, pubkey: &[u8]) {
        eprintln!("handle_theyare");

        let data = serialize_response("i already NEW that");
        self.conns[token].write_message(event_loop, data);
    }
}

impl Handler for Server {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self,
        event_loop: &mut EventLoop<Server>, token: Token, events: EventSet) {

        if events.is_error() {
            if token == self.token {
                println!("server got error");
                event_loop.shutdown();
            } else {
                println!("connection on {:?} got error", token);
                if let Some(name) = self.conns[token].name.clone() {
                    self.name_quits(&name);
                }
                self.conns.remove(token);
            }

            return;
        } 

        if events.is_hup() {
            if token == self.token {
                println!("server got hup");
                event_loop.shutdown();
            } else {
                println!("connection on {:?} got hup", token);
                if let Some(name) = self.conns[token].name.clone() {
                    self.name_quits(&name);
                }
                self.conns.remove(token);
            }

            return;
        }

        if events.is_writable() {
            assert!(token != self.token);

            self.conns[token].sock.write().unwrap();

            if self.conns[token].sock.outbound_queue_len() == 0 {
                event_loop.reregister(self.conns[token].sock.inner(), token,
                    EventSet::all() ^ EventSet::writable(),
                    PollOpt::empty()).unwrap();
            }
        }

        if events.is_readable() {
            if token == self.token {
                self.accept(event_loop);
            } else {
                self.forward(event_loop, token);
            }
        }
    }
}
