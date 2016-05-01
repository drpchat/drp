#![feature(lookup_host)]

extern crate mio;
extern crate bytes;
extern crate nix;
extern crate rustc_serialize;
extern crate sodiumoxide;

extern crate capnp;
extern crate capnp_nonblock;

#[macro_use]
extern crate drp;

use capnp::message::{Reader, ReaderSegments, ReaderOptions};
use capnp_nonblock::MessageStream;

use drp::message;
use drp::util::*;

use mio::*;
use mio::tcp::TcpStream;
use mio::unix::PipeReader;

use rustc_serialize::hex::*;
use sodiumoxide::crypto::box_;
use sodiumoxide::crypto::box_::curve25519xsalsa20poly1305::*;

use std::os::unix::io::FromRawFd;
use std::io::{Read, Write};
use std::io;

use std::fs::File;

use std::net::{SocketAddr, lookup_host};

use std::str::{from_utf8};

use std::env;

// Setup some tokens to allow us to identify which event is
// for which socket.
const STDIN: Token = Token(0);
const SERVCONN: Token = Token(2);

// Define a handler to process the events
struct Client {
    pipe: PipeReader,
    inbuf: Vec<u8>,
    seckey: SecretKey,
    pubkey: PublicKey,
    connection: MessageStream<TcpStream>,
}

impl Handler for Client {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Client>, token: Token, event: EventSet) {
        if token == STDIN {
            if event.is_hup() {
                eprintln!("Event: stdin hup");
                event_loop.shutdown();
                return;
            } else if event.is_error() {
                eprintln!("Event: stdin error");
                event_loop.shutdown();
                return;
            }

            if event.is_readable() {
                let mut buf = vec![0; 512];

                match self.pipe.read(&mut buf) {
                    Ok(n) => self.handle_stdin(&buf, n, event_loop),

                    Err(bad) => {
                        eprintln!("Event: stdin read error {}", bad);
                        event_loop.shutdown();
                    },
                }
            }
        } else {
            if event.is_hup() {
                eprintln!("Event: Server closed connection, exiting");
                event_loop.shutdown();
                return;
            } else if event.is_error() {
                eprintln!("Event: Unknown server error");
                event_loop.shutdown();
                return;
            }
            
            if event.is_readable() {
                if let Some(r) = self.connection.read_message()
                    .unwrap_or_else(|e| panic!("Event: capnproto error: ({})", e)) {

                    self.handle_netin(r);
                } else {
                    //writeln!(std::io::stderr(), "Event: partial message").unwrap();
                }
            }

            if event.is_writable() {
                eprintln!("Event: can write");
                self.connection.write().unwrap();

                if self.connection.outbound_queue_len() == 0 {
                    event_loop.reregister(self.connection.inner(), SERVCONN,
                        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();
                }
            }
        }
    }
}

impl Client {
    fn handle_netin<S>(&mut self, r: Reader<S>) where S: ReaderSegments {
        let msg = r.get_root::<message::Reader>().unwrap();
        match msg.which() {
            Ok(message::Relay(m)) => {
                let mut vbody = Vec::new();
                vbody.extend_from_slice(m.get_body().unwrap());
                let body = from_utf8(&vbody).unwrap();
            
                let mut vsource = Vec::new();
                vsource.extend_from_slice(m.get_source().unwrap());
                let source = from_utf8(&vsource).unwrap();
            
                println!("<{}> {}", source, body);
            },
            Ok(message::Response(m)) => {
                let mut vbody = Vec::new();
                vbody.extend_from_slice(m.get_body().unwrap());
                let body = from_utf8(&vbody).unwrap();
                println!("-!- {}", body);
            },
            Ok(message::Theyare(m)) => {
                let mut vname = Vec::new();
                vname.extend_from_slice(m.get_name().unwrap());
                let name = from_utf8(&vname).unwrap();
            
                let mut vpubkey = Vec::new();
                vpubkey.extend_from_slice(m.get_pubkey().unwrap());
                let pubkey = from_utf8(&vpubkey).unwrap();
            
                println!("-!- Pubkey for {}: {}", name, pubkey);
            },
            Ok(_) => {
                println!("no relay?");
            },
            Err(e) => {
                panic!("Client: network message error");
            },
        }
    }

    // len is the amount of the buffer we actually filled up
    fn handle_stdin(&mut self, buf: &Vec<u8>, len: usize, event_loop: &mut EventLoop<Client>) {
        for i in 0..len {
            match buf[i] {
                b'\n' => { // this is what return does ?
                    let inputs = self.inbuf.clone();
                    let inputs: Vec<&[u8]> = inputs.splitn(3, |x| *x == 32).collect();

                    let cmd = inputs[0];
                    let target = inputs[1];
                    
                    let data = match cmd {
                        b"/join" | b"/j" => {
                            println!("Channel Joined."); // Make it show the channel name
                            serialize_join(target)
                            },
                        b"/part" | b"/p" => {
                            println!("Channel Parted."); // Make it show the channel name
                            serialize_part(target)
                            },
                        b"/msg" | b"/m" | b"/send" => {
                            eprintln!("Message Sent.");
                            let body = inputs[2];
                            serialize_send(target, body)
                            },
                        b"/whois" | "/w" => {
                            serialize_whois(target)
                            },
                        _ => {
                            println!("Sending message to {}", String::from_utf8(Vec::from(cmd)).unwrap());
                            let mut body = Vec::from(target);
                            let target = cmd;
                            body.extend_from_slice(inputs[2]);
                            serialize_send(target, &body)
                        }
                    };

                    self.connection.write_message(data).unwrap();

                    event_loop.reregister(self.connection.inner(), SERVCONN,
                        EventSet::all(), PollOpt::empty()).unwrap();

                    self.inbuf.clear();
                },
                b'\x7f' => { // backspace is delete on linux :<
                    self.inbuf.pop();
                },
                _ => {
                    self.inbuf.push(buf[i]);
                },
            }
        }
    }
}

// TODO this function is a piece of fucking shit
fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    TcpStream::connect(
        &SocketAddr::new(
        lookup_host(host).unwrap().next().unwrap().unwrap().ip()
        ,port)
        )
}

fn main() {
    let nick = env::args().nth(1).unwrap();
    let ip = env::args().nth(2).unwrap();

    // Create an event loop
    let mut event_loop = EventLoop::new().unwrap();

    // register stdin
    let sock = unsafe { PipeReader::from_raw_fd(0) };
    event_loop.register(&sock, STDIN,
        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();

    // register and connect to server
    // Outofthy.me: 104.131.118.79
    let serv_conn = match connect(&ip, 8765) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Remote server not running: {}", e);
            return;
        },
    };    

    let mut serv_conn = MessageStream::new(serv_conn, ReaderOptions::default());
    
    // Load in public and secret keys
    let mut pk = File::open("./pk.key").unwrap();
    let mut pkeys = String::new();
    pk.read_to_string(&mut pkeys).unwrap();
    let pk = pkeys.from_hex().unwrap();
    let pk = PublicKey::from_slice(&pk).unwrap();
    
    let mut sk = File::open("./sk.key").unwrap();
    let mut skeys = String::new();
    sk.read_to_string(&mut skeys).unwrap();
    let sk = skeys.from_hex().unwrap();
    let sk = SecretKey::from_slice(&sk).unwrap();

    // Send Register with public key and nick
    let data = serialize_register(nick.into_bytes().as_slice());
    if let Err(e) = serv_conn.write_message(data) {
        eprintln!("Remote server not running: {}", e);
        return; // TODO But why though?
    };

    event_loop.register(serv_conn.inner(), SERVCONN,
        EventSet::all(), PollOpt::empty()).unwrap();
        
    // Start handling events
    let mut handler = Client { pipe: sock,
                               inbuf: Vec::new(),
                               seckey: sk,
                               pubkey: pk,
                               connection: serv_conn, };

    writeln!(std::io::stderr(), "Main Loop: Connected & Running").unwrap();
    event_loop.run(&mut handler).unwrap();

}
