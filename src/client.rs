#![feature(lookup_host)]
#![feature(io)]

extern crate ncurses;
extern crate mio;
extern crate bytes;
extern crate nix;

extern crate capnp;
extern crate capnp_nonblock;

extern crate drp;

use capnp::message::{Reader, ReaderSegments, ReaderOptions};
use capnp_nonblock::MessageStream;

use drp::message;
use drp::util::*;

use ncurses::*;

use mio::*;
use mio::tcp::TcpStream;
use mio::unix::PipeReader;

use std::os::unix::io::FromRawFd;
use std::io::{Read, Write};

use std::net::{SocketAddr, lookup_host};

use std::collections::VecDeque;
use std::str::{from_utf8};

use std::env;

// Setup some tokens to allow us to identify which event is
// for which socket.
const STDIN: Token = Token(0);
const FOONETIC: Token = Token(2);

fn draw_scroll(scroll: &VecDeque<Vec<u8>>) {
    let mut i = LINES - 2;
    for line in scroll.iter() {
        i -= line.len() as i32 / COLS + 1;

        if i < 0 { break }

        for (j, line) in line.chars().map(|x| x.unwrap())
                                     .collect::<Vec<char>>()
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
    inbuf: Vec<u8>,

    connection: MessageStream<TcpStream>,
    scroll: VecDeque<Vec<u8>>,
    outbuf: Vec<u8>,
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
                if let Some(r) = self.connection.read_message()
                    .unwrap_or_else(|e| panic!("fuck ({})", e)) {

                    self.handle_netin(r);
                } else {
                    writeln!(std::io::stderr(), "not really :(").unwrap();
                }
            }

            if event.is_writable() {
                writeln!(std::io::stderr(), "gotta write fast").unwrap();
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
    fn handle_netin<S>(&mut self, r: Reader<S>) where S: ReaderSegments {
        let msg = r.get_root::<message::Reader>().unwrap();

        if let Ok(message::Relay(m)) = msg.which() {
            let mut v = Vec::new();
            v.extend_from_slice(m.get_body().unwrap());
            self.scroll.push_front(v);

            draw_scroll(&self.scroll);
        } else {
            panic!("baaad girrl");
        }
    }

    // len is the amount of the buffer we actually filled up
    fn handle_stdin(&mut self, buf: &Vec<u8>, len: usize, event_loop: &mut EventLoop<Client>) {
        for i in 0..len {
            writeln!(std::io::stderr(), "key: {:?}", buf[i]).unwrap();
            match buf[i] {
                b'\r' => { // this is what return does ?
                    self.scroll.push_front(self.inbuf.clone());
                    clear();
                    draw_scroll(&self.scroll);

                    let guys = self.inbuf.clone();
                    let guys: Vec<&[u8]> = guys.splitn(3, |x| *x == 32).collect();

                    let recept = guys[0];
                    //writeln!(std::io::stderr(), "recept: {:?}", recept).unwrap();
                    println!("recept: {:?}", recept);

                    let body = guys[1];

                    let data = serialize_send(recept, body);
                    self.connection.write_message(data).unwrap();

                    event_loop.reregister(self.connection.inner(), FOONETIC,
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

        mv(LINES - 1, self.inbuf.len() as i32);
        for _ in self.inbuf.len()..COLS as usize {
            printw(" ");
        }
        mvprintw(LINES - 1, 0, from_utf8(self.inbuf.as_slice()).unwrap());
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
    let nick = env::args().nth(1).unwrap();

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

    let mut free_irc = MessageStream::new(free_irc, ReaderOptions::default());

    let data = serialize_register(nick.into_bytes().as_slice());
    free_irc.write_message(data).unwrap();

    event_loop.register(free_irc.inner(), FOONETIC,
        EventSet::all(), PollOpt::empty()).unwrap();

    // Start handling events
    let mut handler = Client { pipe: sock, //foon: irc,
                               inbuf: Vec::new(), outbuf: Vec::new(),
                               connection: free_irc,
                               scroll: VecDeque::new(), };

    draw_scroll(&handler.scroll);

    writeln!(std::io::stderr(), "bluhhhhhhhhhhhh").unwrap();
    event_loop.run(&mut handler).unwrap();

    // more ncurses bullshit
    endwin();

    println!("we made it to the end");
}
