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

    names: HashMap<Vec<u8>, Token>,
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
            names: HashMap::new(),
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
        if let Some(r) = self.conns[token].sock.read_message()
            .unwrap_or_else(|e| { println!("{:?} (oh no)", e); None }) {

            let msg = r.get_root::<message::Reader>().unwrap();
            println!("got a msg");

            match msg.which() {
                Ok(message::Register(m)) => {
                    let mut conn = &mut self.conns[token];
                    conn.name = {
                        let mut v = Vec::new();
                        v.extend_from_slice(m.get_name().unwrap());
                        Some(v)
                    };

                    self.names.insert(conn.name.clone().unwrap(), token);
                },
                Ok(message::Send(m)) => {
                    let name = {
                        let ref name = self.conns[token].name;
                        name.as_ref().unwrap().clone()
                    };

                    let dest = m.get_dest().unwrap();
                    println!("dest: {:?}", dest);
                    let token = *self.names.get(dest)
                        .expect("couldn't resolve dest");
                    let data = serialize_relay(name.as_slice(),
                        m.get_dest().unwrap(),
                        m.get_body().unwrap());

                    self.conns[token].sock.write_message(data).unwrap();
                    event_loop.reregister(self.conns[token].sock.inner(), token,
                        EventSet::all(), PollOpt::empty()).unwrap();
                },
                Ok(message::Relay(m)) => {
                    let name = {
                        let name = &self.conns[token].name;
                        name.as_ref().unwrap().clone()
                    };

                    let token = *self.names.get(m.get_dest().unwrap()).unwrap();
                    let data = serialize_relay(m.get_source().unwrap(),
                        m.get_dest().unwrap(), m.get_body().unwrap());

                    let mut sock = &mut self.conns[token].sock;
                    sock.write_message(data).unwrap();
                    event_loop.reregister(sock.inner(), token,
                        EventSet::all(), PollOpt::empty()).unwrap();
                },
                Err(e) => println!("fail in ford: {:?}", e),
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
