#![feature(lookup_host)]
#![feature(io)]
#![feature(const_fn)]

extern crate mio;
extern crate bytes;
extern crate nix;
extern crate rustc_serialize;
extern crate sodiumoxide;
extern crate libc;

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

use libc::{free, c_int, c_char, c_void};
use std::ffi::CStr;

use std::ptr::null;

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
use std::process::exit;

#[link(name = "readline")] // not quite fully identical to libedit, it seems:
                           // rl_copy_text and friends are only here
extern {
    fn rl_callback_handler_install(prompt: *const c_char,
        lhandler: extern fn(*const c_char));
    fn rl_callback_read_char();

    fn rl_copy_text(start: c_int, end: c_int) -> *const c_char;
    fn rl_delete_text(start: c_int, end: c_int);
    fn rl_insert_text(text: *const c_char) -> i32;

    fn rl_redisplay();

    static rl_end: c_int;
}

static mut read_line: *const c_char = null();
extern fn stdinput_raw(line: *const c_char) {
    unsafe {
        free(read_line as *mut c_void);
        read_line = line;
    }
}

// Setup some tokens to allow us to identify which event is
// for which socket.
const STDIN: Token = Token(0);
const SERVCONN: Token = Token(2);

// Define a handler to process the events
struct Client {
    pipe: PipeReader,
    inbuf: String,
    seckey: SecretKey,
    //pubkey: PublicKey,
    keys: HashMap<String, PrecomputedKey>,
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

                unsafe {
                    rl_callback_read_char();
                    if !read_line.is_null() {
                        let mut l = Vec::from(CStr::from_ptr(read_line).to_bytes());
                        l.push(b'\n');
                        self.stdinput(&l, event_loop);
                        read_line = null();
                    }
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
    fn handle_relay(&mut self, source: &str, dest: &str, 
        body: Vec<u8>, nonce: Option<&[u8]>) {
        let body = if let Some(nonce) = nonce {
            println!("Decrypting...");
            let prekey = self.keys.get(source).unwrap();
            box_::open_precomputed(&body, 
                &Nonce::from_slice(nonce).unwrap(), &prekey).unwrap()
        } else {
            body
        };
        println!("<{}> {}: {}", source, dest,
            String::from_utf8_lossy(&body),
        );
    }

    fn handle_response(&mut self, body: &str) {
        println!("-!- {}", body);
    }
    
    fn handle_theyare(&mut self, name: &str, pubkey: &[u8]) {
        let pubkey = PublicKey::from_slice(pubkey).unwrap();
        let prekey = box_::precompute(&pubkey, &self.seckey);
        self.keys.insert(String::from(name), prekey);
        println!("-!- Key for {}:\n{}", name, 
            pubkey.0.to_hex());
    }

    fn send_msg(&mut self, target: &str, body: &[u8]) -> 
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
        // make prog1 macro later
        let buf = unsafe {
            let buf = rl_copy_text(0, rl_end);
            rl_delete_text(0, rl_end);
            rl_redisplay();
            buf
        };

        match deserialize(&r).unwrap() {
            Message::Relay { source, dest, body, nonce } =>
                self.handle_relay(source, dest, Vec::from(body), nonce),
            Message::Response { body } =>
                self.handle_response(body),
            Message::Theyare { name, pubkey } =>
                self.handle_theyare(name, pubkey),
            _ => (),
        }    

        unsafe {
            rl_callback_handler_install(&0, stdinput_raw);
            rl_insert_text(buf);
            rl_redisplay();
        }
    }

    // len is the amount of the buffer we actually filled up
    fn stdinput(&mut self, buf: &Vec<u8>, event_loop: &mut EventLoop<Client>) {
        let mut buf = buf.chars();
        while let Some(Ok(c)) = buf.next() {
            match c {
                '\n' => { // Successfully reached end of line
                    let inputs = self.inbuf.clone();
                    let inputs: Vec<&str> = 
                        inputs.splitn(3, |x| x == ' ').collect();

                    let cmd = inputs[0];
                    let target = inputs[1];
                    
                    let data = match cmd {
                        "/join" | "/j" => {
                            println!("Joining {}", target);
                            serialize_join(target)
                        },
                        "/part" | "/p" => {
                            println!("Leaving {}", target);
                            serialize_part(target)
                        },
                        "/whois" | "/w" => {
                            eprintln!("Whois Sent.");
                            serialize_whois(target)
                        },
                        "/msg" | "/m" | "/send" => {
                            eprintln!("Message Sent.");
                            self.send_msg(target, inputs[2].as_bytes())
                        },
                        _ => { // Assume you want a message in form <target> body
                            println!("Sending message to {}", cmd);
                            let body = String::from(target) + inputs[2];
                            self.send_msg(cmd, (body).as_bytes())
                        },
                    };
                    // Send the resulting protocol message to the server
                    self.connection.write_message(data).unwrap();
                },
                _ => { // Non end of line character, add to buffer
                    self.inbuf.push(c);
                },
            }
        }
    
        if buf.next().is_some() {
            println!("-?- Wtf was that?");
        }
        event_loop.reregister(self.connection.inner(), SERVCONN,
            EventSet::all(), PollOpt::empty()).unwrap();

        self.inbuf.clear();
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

    // register stdin callback with readline
    unsafe { rl_callback_handler_install(&0, stdinput_raw); }

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

    let sk = File::open("./sk.key").unwrap();
    let sk : String = sk.chars().filter_map(|x| x.ok()).collect();
    let sk = sk.from_hex().unwrap();
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
                               inbuf: String::new(),
                               seckey: sk,
                               //pubkey: pk,
                               keys: HashMap::new(),
                               connection: serv_conn, };

    writeln!(std::io::stderr(), "Main Loop: Connected & Running").unwrap();
    event_loop.run(&mut handler).unwrap();

}
