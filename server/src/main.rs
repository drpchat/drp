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

struct Server {
    token: Token,
    sock: TcpListener,
    conns: Slab<TcpStream>,
}

impl Server {
    fn new() -> Server {
        Server {
            token: Token(1),
            sock: TcpListener::bind(&FromStr::from_str("127.0.0.1:8765").unwrap()).unwrap(),
            conns: Slab::new_starting_at(Token(2), 128),
        }
    }
}

impl Handler for Server {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Server>, token: Token, events: EventSet) {
        if events.is_error() {
            panic!("is_error");
        }

        if events.is_hup() {
            panic!("it's over!");
        }

        if events.is_writable() {
            println!("testo");
            // not yet care abouted
        }

        if events.is_readable() {
            println!("we got a read!");
            if self.token == token {
                // accept a new connection
                let sock = match self.sock.accept() {
                    Ok(s) => {
                        match s {
                            Some(so) => so.0,
                            None => panic!("sock error of kind b"),
                        }
                    },
                    Err(_) => {
                        panic!("sock error of kind a");
                    },
                };

                match self.conns.insert(sock) {
                    Ok(token) => {
                        // register the guy
                        event_loop.register(&self.conns[token], token,
                            EventSet::all() ^ EventSet::writable(),
                            PollOpt::empty()).unwrap();
                    },
                    Err(_) => {
                        panic!("failed to make new connect");
                    },
                }
            } else {
                // give it to the guy
                let mut recv_buf = ByteBuf::mut_with_capacity(2048);
                match self.conns[token].try_read_buf(&mut recv_buf) {
                    Ok(Some(_)) => {
                        for c in recv_buf.flip().chars() {
                            print!("{}", c.unwrap());
                        }
                    },
                    Ok(None) | Err(_) => {
                        // nope
                        ()
                    },
                }
            }
        }
    }
}

fn main() {
    // Create an event loop
    let mut handler = Server::new();

    let mut event_loop = EventLoop::new().unwrap();

    event_loop.register(&handler.sock, handler.token, EventSet::readable(), PollOpt::empty()).unwrap();
    event_loop.run(&mut handler).unwrap();
}
