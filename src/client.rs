#![feature(lookup_host)]
#![feature(io)]

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

//use drp::message;
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
use std::collections::HashMap;

use std::net::{SocketAddr, lookup_host};

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
    //pubkey: PublicKey,
    keys: HashMap<Vec<u8>, PrecomputedKey>,
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
                    Ok(n) => self.stdinput(&buf, n, event_loop),
                    Err(e) => {
                        eprintln!("Event: stdin read error {}", e);
                        event_loop.shutdown();
                    },
                }
            }
        } else {
            if event.is_hup() {
                println!("Server closed connection, exiting...");
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
                    self.netinput(r);
                } else {
                    //eprintln!("Event: partial message");
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
    fn handle_relay(&mut self, source: &[u8], 
        dest: &[u8], body: Vec<u8>, nonce: Option<&[u8]>) {
        let body = if let Some(nonce) = nonce {
            println!("Decrypting...");
            let prekey = self.keys.get(source).unwrap();
            box_::open_precomputed(&body, 
                &Nonce::from_slice(nonce).unwrap(), &prekey).unwrap()
        } else {
            body
        };
        println!("<{}> {}: {}", 
            String::from_utf8_lossy(source), 
            String::from_utf8_lossy(dest),
            String::from_utf8_lossy(&body),
        );
    }
    
    fn handle_response(&mut self, body: &[u8]) {
        println!("-!- {}", String::from_utf8_lossy(body));
    }
    
    fn handle_theyare(&mut self, name: Vec<u8>, pubkey: &[u8]) {
        let pubkey = PublicKey::from_slice(pubkey).unwrap();
        let prekey = box_::precompute(&pubkey, &self.seckey);
        self.keys.insert(name.clone(), prekey);
        println!("-!- Key for {}:\n{}", String::from_utf8_lossy(&name), 
            pubkey.0.to_hex());
    }

    fn send_msg(&mut self, target: &[u8], body: &[u8]) -> 
        capnp::message::Builder<capnp::message::HeapAllocator> {
        if self.keys.contains_key(target) {
            println!("Sending encrypted message...");
            let nonce = box_::gen_nonce();
            let body = &box_::seal_precomputed(body, &nonce, 
                &self.keys.get(target).unwrap());
            let nonce: &[u8] = &nonce.0;
            return serialize_send(target, body, Some(nonce))
        } else {
            return serialize_send(target, body, None)
        }
    }
    
    fn netinput<S>(&mut self, r: Reader<S>) where S: ReaderSegments {
        match deserialize(&r).unwrap() {
            Message::Relay { source, dest, body, nonce } =>
                self.handle_relay(source, dest, Vec::from(body), nonce),
            Message::Response { body } =>
                self.handle_response(body),
            Message::Theyare { name, pubkey } =>
                self.handle_theyare(Vec::from(name), pubkey),
            _ => (),
        }    
    }

    // len is the amount of the buffer we actually filled up
    fn stdinput(&mut self, buf: &Vec<u8>, len: usize, event_loop: &mut EventLoop<Client>) {
        for i in 0..len {
            match buf[i] {
                b'\n' => { // this is what return does ?
                    let inputs = self.inbuf.clone();
                    let inputs: Vec<&[u8]> = 
                        inputs.splitn(3, |x| *x == 32).collect();

                    let cmd = inputs[0];
                    let target = inputs[1];
                    
                    let data = match cmd {
                        b"/join" | b"/j" => {
                            println!("Joining {}",
                                String::from_utf8_lossy(target));
                            serialize_join(target)
                        },
                        b"/part" | b"/p" => {
                            println!("Leaving {}",
                                String::from_utf8_lossy(target));
                            serialize_part(target)
                        },
                        b"/whois" | b"/w" => {
                            eprintln!("Whois Sent.");
                            serialize_whois(target)
                        },
                        b"/msg" | b"/m" | b"/send" => {
                            eprintln!("Message Sent.");
                            self.send_msg(target, inputs[2])
                        },
                        _ => {
                            println!("Sending message to {}", 
                                String::from_utf8_lossy(cmd));
                            let mut body = Vec::from(target);
                            body.extend_from_slice(inputs[2]);
                            self.send_msg(cmd, &body)
                        },
                    };

                    self.connection.write_message(data).unwrap();

                    event_loop.reregister(self.connection.inner(), SERVCONN,
                        EventSet::all(), PollOpt::empty()).unwrap();

                    self.inbuf.clear();
                },
                b'\x7f' => { // backspace is delete on linux
                    self.inbuf.pop();
                },
                _ => {
                    self.inbuf.push(buf[i]);
                },
            }
        }
    }
}

// TODO This function is am awful mess
fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    TcpStream::connect(
        &SocketAddr::new(
        lookup_host(host).unwrap().next().unwrap().unwrap().ip()
        ,port)
        )
}

fn main() {
    let nick = env::args().nth(1).unwrap();
    let server = env::args().nth(2).unwrap();

    // Create an event loop
    let mut event_loop = EventLoop::new().unwrap();

    // Register stdin event handler
    let sock = unsafe { PipeReader::from_raw_fd(0) };
    event_loop.register(&sock, STDIN,
        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();

    // Connect to server
    let serv_conn = match connect(&server, 8765) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Remote server error? {}", e);
            return;
        },
    };    

    // Load in public and secret keys
    let pk = File::open("./pk.key").unwrap();
    let pk : String = pk.chars().filter_map(|x| x.ok()).collect();
    let pkstr = pk.from_hex().unwrap();
    //let pk = PublicKey::from_slice(&pkstr).unwrap();
    
    let mut sk = File::open("./sk.key").unwrap();
    let mut skstr = String::new();
    sk.read_to_string(&mut skstr).unwrap();
    let sk = skstr.from_hex().unwrap();
    let sk = SecretKey::from_slice(&sk).unwrap();

    // Register server event handler
    let mut serv_conn = MessageStream::new(serv_conn, ReaderOptions::default());
    event_loop.register(serv_conn.inner(), SERVCONN, 
        EventSet::all(), PollOpt::empty()).unwrap();

    // Send Register message with public key and nick
    let data = serialize_register(&nick, &pkstr);
    if let Err(e) = serv_conn.write_message(data) {
        eprintln!("Remote server not running: {}", e);
        return; // TODO But why though?
    };
            
    // Create handler object and run it
    let mut handler = Client { pipe: sock,
                               inbuf: Vec::new(),
                               seckey: sk,
                               //pubkey: pk,
                               keys: HashMap::new(),
                               connection: serv_conn, };

    writeln!(std::io::stderr(), "Main Loop: Connected & Running").unwrap();
    event_loop.run(&mut handler).unwrap();

}
