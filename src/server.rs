extern crate mio;
extern crate bytes;
extern crate nix;

extern crate capnp;
extern crate capnp_nonblock;

extern crate drp;

use mio::*;
use mio::tcp::{TcpListener, TcpStream};
use mio::util::Slab;

use std::str::FromStr;
use std::collections::HashMap;

use capnp::message::{Builder, HeapAllocator, ReaderOptions};
use capnp_nonblock::MessageStream;

use drp::util::*;

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
    name: Option<Vec<u8>>,

    sock: MessageStream<TcpStream>,
    token: Token,
    //interest: EventSet,
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

struct Server {
    token: Token,
    sock: TcpListener,
    conns: Slab<Connection>,

    names: HashMap<Vec<u8>, Token>,
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
            name: None,
            sock: MessageStream::new(sock, ReaderOptions::default()),
            token: token }) {

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
                Message::Register { name } => {
                    let mut conn = &mut self.conns[token];
                    let name = Vec::from(name);

                    if self.names.contains_key(&name) {
                        let data = serialize_response(b"dude ur not them");
                        conn.write_message(event_loop, data);
                    } else {
                        self.names.insert(name.clone(), token);
                        conn.name = Some(name);
                    }
                },
                Message::Send { dest, body } => {
                    let name = {
                        let ref name = self.conns[token].name;
                        name.as_ref().unwrap().clone()
                    };

                    println!("dest: {:?}", dest);

                    if let Some(chanlist) = self.channels.get(dest) {
                        for dest in chanlist {
                            println!("{:?}", dest);
                            let token = *self.names.get(dest)
                                .expect("couldn't resolve dest");

                            let data = serialize_relay(name.as_slice(),
                                dest, body);

                            self.conns[token].write_message(event_loop, data);
                        }
                    } else {
                        let token = *self.names.get(dest)
                            .expect("couldn't resolve dest");

                        let data = serialize_relay(name.as_slice(),
                            dest, body);

                        self.conns[token].write_message(event_loop, data);
                    }
                },
                Message::Relay { source, dest, body } => {
                    let token = *self.names.get(dest).unwrap();
                    let data = serialize_relay(source, dest, body);

                    self.conns[token].write_message(event_loop, data);
                },
                Message::Join { channel } => {
                    let name = {
                        let ref name = self.conns[token].name;
                        name.as_ref().unwrap().clone()
                    };

                    let mut c = Vec::new();
                    c.extend_from_slice(channel);
                    
                    let mut chans = &mut self.channels;

                    if chans.get_mut(&c).map(|c| c.push(name.clone())).is_none() {
                        chans.insert(c, vec![name]);
                    }
                },
                Message::Part { channel } => {
                    let name = {
                        let ref name = self.conns[token].name;
                        name.as_ref().unwrap().clone()
                    };

                    let mut c = Vec::new();
                    c.extend_from_slice(channel);

                    let mut chans = &mut self.channels;

                    match chans.get_mut(&c) {
                        Some(c) => {
                            if let Ok(i) = c.binary_search(&name) {
                                c.remove(i);
                            }
                        },
                        None => (),
                    }
                },
                Message::Response { body } => {
                    let data = serialize_response(b"no ur a client");
                    self.conns[token].write_message(event_loop, data);
                }
            }
        } else {
            println!("nope, let's go");
        }
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
                    EventSet::readable() | EventSet::error() | EventSet::hup(),
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
