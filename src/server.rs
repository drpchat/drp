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

use capnp::message::{ReaderOptions};
use capnp_nonblock::MessageStream;

use drp::message;
use drp::util::*;

fn main() {
    // Create an event loop
    let mut handler = Server::new();

    let mut event_loop = EventLoop::new().unwrap();

    event_loop.register(&handler.sock, handler.token, EventSet::readable(), PollOpt::empty()).unwrap();
    event_loop.run(&mut handler).unwrap();
}

struct Server {
    token: Token,
    sock: TcpListener,
    conns: Slab<Connection>,
}

struct Connection {
    name: Option<Vec<u8>>,

    sock: MessageStream<TcpStream>,
    token: Token,
}

impl Server {
    fn new() -> Server {
        Server {
            token: Token(1),
            sock: TcpListener::bind(&FromStr::from_str("0.0.0.0:8765").unwrap()).unwrap(),
            conns: Slab::new_starting_at(Token(2), 128),
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

        match self.conns.insert_with(|token| { Connection {
            name: None,
            sock: MessageStream::new(sock, ReaderOptions::default()),
                token: token } }) {
            Some(token) => {
                // register the guy
                event_loop.register(self.conns[token].sock.inner(), token,
                    EventSet::readable(),
                    PollOpt::empty()).unwrap();
            },
            None => {
                println!("failed to make new connect");
            },
        }
    }

    fn forward(&mut self, event_loop: &mut EventLoop<Server>, token: Token) {
        // give it to the guy
        match self.conns[token].sock.read_message() {
            Ok(Some(r)) => {
                let msg = r.get_root::<message::Reader>().unwrap();
                println!("got a msg");

                match msg.which() {
                    Ok(message::Register(m)) => {
                        self.conns[token].name = {
                            let mut v = Vec::new();
                            v.extend_from_slice(m.get_name().unwrap());
                            Some(v)
                        };

                        println!("DID: register {:?}", self.conns[token].name);
                    },
                    Ok(message::Send(m)) => {
                        let name = {
                            let ref name = self.conns[token].name;
                            name.as_ref().unwrap().clone()
                        };

                        for conn in self.conns.iter_mut() {
                            let data = serialize_relay(name.as_slice(),
                                m.get_dest().unwrap(),
                                m.get_body().unwrap());

                            conn.sock.write_message(data).unwrap();
                            event_loop.reregister(conn.sock.inner(), token,
                                EventSet::all(), PollOpt::empty()).unwrap();

                            println!("DID: write to a conn");
                        }
                    },
                    Ok(message::Relay(m)) => {
                        for conn in self.conns.iter_mut() {
                            let data = serialize_relay(m.get_source().unwrap(),
                                m.get_dest().unwrap(), m.get_body().unwrap());

                            conn.sock.write_message(data).unwrap();
                            event_loop.reregister(conn.sock.inner(), token,
                                EventSet::all(),
                                PollOpt::empty()).unwrap();
                            println!("DID: write to a conn");
                        }
                    },
                    Err(e) => println!("fail in ford: {:?}", e),
                }
            },
            Ok(None) => {
                println!("nope, let's go");
                // waiting and shit
            }
            Err(e) => {
                println!("{:?} (oh no)", e);
            }
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
                    EventSet::readable(), PollOpt::empty()).unwrap();
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
