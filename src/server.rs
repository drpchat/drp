extern crate mio;
extern crate bytes;
extern crate nix;

extern crate capnp;
extern crate capnp_nonblock;

#[macro_use]
extern crate drp;

use mio::*;
use mio::tcp::{TcpListener, TcpStream};
use mio::util::Slab;

use std::str::FromStr;
use std::collections::HashMap;

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

    name: Option<Vec<u8>>,
    pubkey: Option<Vec<u8>>,
    channels: Vec<Vec<u8>>,
}

impl Connection {
    // async write to our sock, then reregister for writable events.  the mio
    // handler will unset the writable event once our message is actually sent
    fn write_message(&mut self, event_loop: &mut EventLoop<Server>,
        msg: Builder<HeapAllocator>) {

        println!("writing");

        self.sock.write_message(msg).unwrap();
        event_loop.reregister(self.sock.inner(), self.token,
            EventSet::all(), PollOpt::empty()).unwrap();
    }
}

//impl Drop for Connection {
//    fn drop(&mut self) {
//        println!("drop it!!");
//    }
//}

struct Server {
    token: Token,
    sock: TcpListener,

    names: HashMap<Vec<u8>, Token>,
    conns: Slab<Connection>,
    channels: HashMap<Vec<u8>, Vec<Vec<u8>>>,
}

impl Server {
    fn new() -> Server {
        Server {
            token: Token(1),
            sock: TcpListener::bind(&FromStr::from_str("0.0.0.0:8765").unwrap()).unwrap(),
            conns: Slab::new_starting_at(Token(2), 128),
            names: HashMap::new(),
            channels: HashMap::new(),
        }
    }

    // a new user is trying to register - add them to the db
    fn add_name(&mut self, token: Token, name: &Vec<u8>, pubkey: &Vec<u8>)
        -> Option<()> {

        if self.names.contains_key(name) {
            return None
        }

        self.names.insert(name.clone(), token);
        self.conns[token].name = Some(name.clone()); // TODO deal with nick changes
        self.conns[token].pubkey = Some(pubkey.clone()); // TODO deal with nick changes

        return Some(())
    }

    // a user is joining a channel
    fn name_joins(&mut self, name: &Vec<u8>, channel: &Vec<u8>) -> Option<()> {
        //if self.channels.contains_key(name) {
        //    return None;
        //}

        // add name to channel
        let chan = self.channels.entry(channel.clone()).or_insert(Vec::new());
        if chan.contains(name) {
            return None
        } else {
            chan.push(name.clone());
        }

        // add channel to name
        let token = self.names[name];
        self.conns[token].channels.push(channel.clone());

        Some(())
    }

    // a user is leaving a channel
    fn name_leaves(&mut self, name: &Vec<u8>, channel: &Vec<u8>) -> Option<()> {
        // remove name from channel
        let mut chans = self.channels.get_mut(channel);
        let mut chans = match chans {
            None => return None,
            Some(chans) => chans,
        };

        if let Ok(i) = chans.binary_search(&name) {
            chans.remove(i);
        }

        // remove channel from name
        let token = self.names[name];
        if let Ok(i) = self.conns[token].channels.binary_search(&channel) {
            self.conns[token].channels.remove(i);
        } else {
            return None;
        }

        Some(())
    }

    // name quits the server: needs to leave all channels and clear name
    fn name_quits(&mut self, name: &Vec<u8>) {
        for channel in self.conns[self.names[name]].channels.clone() {
            self.name_leaves(name, &channel);
        }

        self.names.remove(name);
    }

    fn accept(&mut self, event_loop: &mut EventLoop<Server>) {
        let sock = match self.sock.accept() {
            Ok(s) => {
                match s {
                    Some(so) => so.0,
                    None => {
                        println!("sock error of kind b");
                        return;
                    },
                }
            },
            Err(_) => {
                println!("sock error of kind a");
                return;
            },
        };

        match self.conns.insert_with(|token| Connection {
            sock: MessageStream::new(sock, ReaderOptions::default()),
            token: token,
            name: None, pubkey: None,
            channels: Vec::new() }) {

            Some(token) => {
                // register the guy
                event_loop.register(self.conns[token].sock.inner(), token,
                    EventSet::readable() | EventSet::error() | EventSet::hup(),
                    PollOpt::empty()).unwrap();
            },
            None => {
                println!("failed to make new connect");
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
                Message::Send { dest, body } =>
                    self.handle_send(event_loop, token, dest, body),
                Message::Relay { source, dest, body } =>
                    self.handle_relay(event_loop, token, source, dest, body),
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
            println!("nope, let's go");
        }
    }

    fn handle_register(&mut self, event_loop: &mut EventLoop<Server>,
        token: Token, name: &[u8], pubkey: &[u8]) {
        eprintln!("handle_register");

        self.add_name(token, &Vec::from(name), &Vec::from(pubkey))
            .unwrap_or_else(|| {
            let data = serialize_response(b"dude ur not them");
            self.conns[token].write_message(event_loop, data);
        });
    }

    fn handle_send(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, dest: &[u8], body: &[u8]) {
        eprintln!("handle_sned");

        let name = {
            let mut conn = &mut self.conns[token];
            match conn.name.clone() {
                Some(name) => name,
                None => {
                    let data = serialize_response(b"this isnt 4chan");
                    conn.write_message(event_loop, data);
                    return
                }
            }
        };

        println!("dest: {:?}", dest);

        if let Some(chanlist) = self.channels.get(dest) {
            println!("chanlist: {:?}", chanlist);
            for dest in chanlist {
                println!("putting to {:?}",
                    String::from_utf8(dest.clone()).unwrap());

                let token = *self.names.get(dest)
                    .expect("couldn't resolve dest");

                let data = serialize_relay(name.as_slice(),
                    dest, body);

                self.conns[token].write_message(event_loop, data);
            }
        } else {
            println!("doing a lil guy");
            println!("||| {:?} -> {:?}",
                String::from_utf8(name.clone()).unwrap(),
                String::from_utf8(Vec::from(dest)).unwrap());
            let token = *self.names.get(dest)
                .expect("couldn't resolve dest");

            let data = serialize_relay(name.as_slice(),
                dest, body);

            self.conns[token].write_message(event_loop, data);
        }
    }

    fn handle_relay(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, source: &[u8], dest: &[u8], body: &[u8]) {
        eprintln!("handle_relay");
        let token = *self.names.get(dest).unwrap();
        let data = serialize_relay(source, dest, body);

        self.conns[token].write_message(event_loop, data);
    }

    fn handle_join(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, channel: &[u8]) {
        eprintln!("handle_join");

        if let Some(name) = self.conns[token].name.clone() {
            self.name_joins(&name, &Vec::from(channel)).unwrap_or_else(|| {
                let data = serialize_response(b"you're already there!!");
                self.conns[token].write_message(event_loop, data);
            });
        } else {
            let data = serialize_response(b"who ARE u!?!");
            self.conns[token].write_message(event_loop, data);
        }
    }

    fn handle_part(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, channel: &[u8]) {
        eprintln!("handle_part");
        
        if let Some(name) = self.conns[token].name.clone() {
            self.name_leaves(&name, &Vec::from(channel)).unwrap_or_else(|| {
                let data = serialize_response(b"you're not even there!!");
                self.conns[token].write_message(event_loop, data);
            });
        } else {
            let data = serialize_response(b"who ARE u!?!");
            self.conns[token].write_message(event_loop, data);
        }
    }

    fn handle_response(&mut self, event_loop: &mut EventLoop<Server>,
    token: Token, body: &[u8]) {
        eprintln!("handle_respo");

        let data = serialize_response(b"no ur a client");
        self.conns[token].write_message(event_loop, data);
    }

    fn handle_whois(&mut self, event_loop: &mut EventLoop<Server>,
        token: Token, name: &[u8]) {
        eprintln!("handle_whose");

        if let Some(id) = self.names.get(name) {
            eprintln!(" got name");
            if let Some(pubkey) = self.conns[*id].pubkey.clone() {
                eprintln!(" got pubkey");
                let data = serialize_theyare(name, &pubkey);
                self.conns[token].write_message(event_loop, data);
                return;
            }
        }

        let data = serialize_response(b"they don't exist");
        self.conns[token].write_message(event_loop, data);
    }

    fn handle_theyare(&mut self, event_loop: &mut EventLoop<Server>,
        token: Token, name: &[u8], pubkey: &[u8]) {
        eprintln!("handle_theyare");

        let data = serialize_response(b"i already NEW that");
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
            println!("gotta write fast");

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
