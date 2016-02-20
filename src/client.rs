#![feature(lookup_host)]

extern crate ncurses;
extern crate mio;
extern crate bytes;
extern crate nix;

extern crate capnp;
extern crate capnp_nonblock;

mod drp_capnp {
    include!(concat!(env!("OUT_DIR"), "/drp_capnp.rs"));
}

use drp_capnp::message;

use capnp::serialize_packed;
use capnp::message::{Builder, ReaderOptions};
use capnp_nonblock::MessageStream;

use ncurses::*;

use mio::*;
use mio::tcp::TcpStream;
use mio::unix::PipeReader;

use bytes::ByteBuf;

use std::os::unix::io::FromRawFd;
use std::io::{BufReader, BufRead, Read, Write};

use std::net::{SocketAddr, lookup_host};

use std::collections::VecDeque;
use std::str::FromStr;

// todo: use bytes instead of string
#[derive(Debug)]
struct Message {
    source: String,
    dest: String,
    body: String,
}

fn read_message<R>(read: &mut R, options: ReaderOptions)
    -> capnp::Result<Message> where R: BufRead {

    serialize_packed::read_message(read, options).and_then(|r| {
        r.get_root::<message::Reader>().and_then(|msg| {
            let source = try!(msg.get_source());
            let dest = try!(msg.get_dest());
            let body = try!(msg.get_body());

            Ok(Message {
                source: String::from_str(source).unwrap(),
                dest: String::from_str(dest).unwrap(),
                body: String::from_str(body).unwrap(),
            })
        })
    })
}

// Setup some tokens to allow us to identify which event is
// for which socket.
const STDIN: Token = Token(0);
const FOONETIC: Token = Token(2);

fn draw_scroll(scroll: &VecDeque<String>) {
    let mut i = LINES - 2;
    for line in scroll.iter() {
        i -= line.len() as i32 / COLS + 1;

        if i < 0 { break }

        for (j, line) in line.chars().collect::<Vec<char>>()
                                     .chunks(COLS as usize).enumerate() {
            mv(i + j as i32, 0);
            for c in line {
                addch(*c as u64);
            }
        }

        clrtoeol();
    }

    mv(LINES - 2, 0);
    for _ in 0..COLS {
        printw("-");
    }

    mv(LINES - 1, 0);
    refresh();
}

// Define a handler to process the events
struct Client {
    pipe: PipeReader,
    inbuf: String,

    connection: MessageStream<TcpStream>,
    scroll: VecDeque<String>,
    outbuf: String,
}

impl Handler for Client {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Client>, token: Token, event: EventSet) {
        if token == STDIN {
            if event.is_hup() {
                writeln!(std::io::stderr(), "stdin hup").unwrap();
                event_loop.shutdown();
                return;
            } else if event.is_error() {
                writeln!(std::io::stderr(), "stdin error").unwrap();
                event_loop.shutdown();
                return;
            }

            if event.is_readable() {
                let mut buf = vec![0; 512];

                match self.pipe.read(&mut buf) {
                    Ok(n) => self.handle_stdin(&buf, n, event_loop),

                    Err(bad) => {
                        writeln!(std::io::stderr(), "bad a! {}", bad).unwrap();
                        event_loop.shutdown();
                    },
                }
            }
        } else {
            if event.is_hup() {
                writeln!(std::io::stderr(), "net hup").unwrap();
                event_loop.shutdown();
                return;
            } else if event.is_error() {
                writeln!(std::io::stderr(), "net error").unwrap();
                event_loop.shutdown();
                return;
            }
            
            if event.is_readable() {
                let mut buf = ByteBuf::mut_with_capacity(2048);

                match self.connection.read_message() {
                    Ok(Some(r)) => {
                        let msg = r.get_root::<message::Reader>().unwrap();

                        let source = msg.get_source().unwrap();
                        let dest = msg.get_dest().unwrap();
                        let body = msg.get_body().unwrap();

                        self.scroll.push_front(format!("{} -> {}: {}",
                            source, dest, body));

                        draw_scroll(&self.scroll);
                    },
                    Ok(None) => {
                        (); // still  wait tin
                    },
                    Err(e) => {
                        panic!("FUCK");
                    }
                }
            }

            if event.is_writable() {
                self.connection.write().unwrap();
                if self.connection.outbound_queue_len() == 0 {
                    event_loop.reregister(self.connection.inner(), FOONETIC,
                        EventSet::readable(), PollOpt::empty()).unwrap();
                }
            }
        }
    }
}

impl Client {
    // len is the amount of the buffer we actually filled up
    fn handle_netin(&mut self, buf: &Vec<u8>, len: usize) {
        for i in 0..len {
            if buf[i] as char == '\n' { continue }

            if buf[i] as char == '\r' {
                let temp = self.outbuf.clone();
                writeln!(std::io::stderr(), "{}", temp).unwrap();

                self.scroll.push_front(temp);
                self.outbuf.clear();
            } else {
                self.outbuf.push(((buf[i] & 0xf0 >> 4) + 97) as char);
                self.outbuf.push((buf[i] & 0x0f + 97) as char);
            }
        }

        clear();
        draw_scroll(&self.scroll);
    }

    // len is the amount of the buffer we actually filled up
    fn handle_stdin(&mut self, buf: &Vec<u8>, len: usize, event_loop: &mut EventLoop<Client>) {
        for i in 0..len {
            writeln!(std::io::stderr(), "key: {:?}", buf[i]).unwrap();
            match buf[i] as char {
                '\r' => { // this is what return does ?
                    self.scroll.push_front(self.inbuf.clone());
                    clear();
                    draw_scroll(&self.scroll);

                    //self.inbuf.push_str("\r\n");

                    let mut data = Builder::new_default();
                    {
                        let mut msg = data.init_root::<message::Builder>();

                        msg.set_source("[Awark");
                        msg.set_dest("Lan");
                        msg.set_body(&self.inbuf);
                    }

                    self.connection.write_message(data).unwrap();

                    event_loop.reregister(self.connection.inner(), FOONETIC,
                        EventSet::all(), PollOpt::empty()).unwrap();

                    //match self.connection.write_all(self.inbuf.as_bytes()) {
                    //    Ok(_) => (),
                    //    Err(bad) => println!("bad d! {:?}", bad),
                    //}

                    self.inbuf.clear();
                },
                '\x7f' => { // backspace is delete on linux :<
                    self.inbuf.pop();
                },
                _ => {
                    self.inbuf.push(buf[i] as char);
                },
            }
        }

        mv(LINES - 1, self.inbuf.len() as i32);
        for _ in self.inbuf.len()..COLS as usize {
            printw(" ");
        }
        mvprintw(LINES - 1, 0, &self.inbuf);
        refresh();
    }
}

// TODO this function is a piece of fucking shit
fn connect(host: &str, port: u16) -> TcpStream {
    TcpStream::connect(
        &
        SocketAddr::new(
        lookup_host(host).unwrap().next().unwrap().unwrap().ip()
        ,port)
    ).unwrap()
}

fn main() {
    // ncurses bullshit
    initscr();
    cbreak();
    clear();

    // Create an event loop
    let mut event_loop = EventLoop::new().unwrap();

    // register stdin
    let sock = unsafe { PipeReader::from_raw_fd(0) };
    event_loop.register(&sock, STDIN,
        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();

    // register freenode
    let free_irc = connect("127.0.0.1", 8765);

    let free_irc = MessageStream::new(free_irc, ReaderOptions::default());

    event_loop.register(free_irc.inner(), FOONETIC,
        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();

    // Start handling events
    let mut handler = Client { pipe: sock, //foon: irc,
                               inbuf: String::new(), outbuf: String::new(),
                               connection: free_irc,
                               scroll: VecDeque::new(), };

    draw_scroll(&handler.scroll);

    writeln!(std::io::stderr(), "bluhhhhhhhhhhhh").unwrap();
    event_loop.run(&mut handler).unwrap();

    // more ncurses bullshit
    endwin();

    println!("we made it to the end");
}
