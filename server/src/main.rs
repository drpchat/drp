#![feature(io)]

extern crate mio;
extern crate bytes;
extern crate nix;

use mio::*;
use mio::tcp::{TcpListener, TcpStream};
use mio::util::Slab;

use bytes::ByteBuf;

use std::io::{Read, Write};
use std::net::SocketAddr;

use std::str::FromStr;

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
    sock: TcpStream,
    messages: Vec<ByteBuf>,
    token: Token,
}

impl Server {
    fn new() -> Server {
        Server {
            token: Token(1),
            sock: TcpListener::bind(&FromStr::from_str("127.0.0.1:8765").unwrap()).unwrap(),
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

        match self.conns.insert_with(|token| { Connection { sock: sock, messages: Vec::new(), token: token } }) {
            Some(token) => {
                // register the guy
                event_loop.register(&self.conns[token].sock, token,
                    EventSet::all(),
                    PollOpt::empty()).unwrap();
            },
            None => {
                println!("failed to make new connect");
            },
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
            println!("testo");

            assert!(token != self.token);
            
            self.conns[token].messages.pop().and_then(|mut msg| {
                self.conns[token].sock.try_write_buf(&mut msg).unwrap()
            });

            if self.conns[token].messages.is_empty() {
                event_loop.reregister(&mut self.conns[token].sock, token,
                    EventSet::readable(), PollOpt::empty()).unwrap();
            }
        }

        if events.is_readable() {
            println!("we got a read!");
            if token == self.token {
                self.accept(event_loop);
            } else {
                // give it to the guy
                let mut recv_buf = ByteBuf::mut_with_capacity(2048);
                match self.conns[token].sock.try_read_buf(&mut recv_buf) {
                    Ok(Some(_)) => {
                        for conn in self.conns.iter_mut() {
                            conn.messages.push(
                                ByteBuf::from_slice(recv_buf.bytes()));
                            event_loop.reregister(&mut conn.sock, conn.token,
                                EventSet::all(), PollOpt::empty()).unwrap();
                        }

                        for c in recv_buf.flip().chars() {
                            print!("{}", c.unwrap());
                        }
                    },
                    Ok(None) => (), // EAGAIN
                    Err(_) => {
                        println!("error while reading");
                        self.conns.remove(token);
                    },
                }
            }
        }
    }
}
