#![feature(lookup_host)]
#![feature(ip_addr)]

#![feature(str_char)]

extern crate ncurses;
extern crate mio;
extern crate nix;

use ncurses::*;

use mio::*;
use mio::tcp::TcpStream;
use mio::unix::PipeReader;

use std::os::unix::io::FromRawFd;
use std::io::{Read, Write};

use std::net::{SocketAddr, lookup_host};

use std::collections::VecDeque;

use nix::sys::signalfd::*;

// Setup some tokens to allow us to identify which event is
// for which socket.
const STDIN: Token = Token(0);
const FOONETIC: Token = Token(2);
const FREENODE: Token = Token(3);

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
    }

    mv(LINES - 2, 0);
    for _ in 0..COLS {
        printw("-");
    }

    mv(LINES - 1, 0);
}

// Define a handler to process the events
struct Client {
    pipe: PipeReader,
    inbuf: String,

    current_conn: usize,
    connections: Vec<(TcpStream, VecDeque<String>, String)>,

    on_ctl: bool,
    ctl_scroll: VecDeque<String>,
}

impl Handler for Client {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Client>, token: Token, event: EventSet) {
        match token {
            STDIN => if event.is_hup() {
                writeln!(std::io::stderr(), "stdin hup").unwrap();
                event_loop.shutdown();
            } else if event.is_error() {
                writeln!(std::io::stderr(), "stdin error").unwrap();
                event_loop.shutdown();
            } else if event.is_readable() {
                let mut buf = vec![0; 512];

                match self.pipe.read(&mut buf) {
                    Ok(n) => self.handle_stdin(&buf, n),

                    Err(bad) => {
                        writeln!(std::io::stderr(), "bad a! {}", bad).unwrap();
                        event_loop.shutdown();
                    },
                }
            },

            Token(conn) => if event.is_hup() {
                writeln!(std::io::stderr(), "net hup").unwrap();
                event_loop.shutdown();
            } else if event.is_error() {
                writeln!(std::io::stderr(), "net error").unwrap();
                event_loop.shutdown();
            } else if event.is_readable() {
                let mut buf = vec![0; 512];

                match self.connections[conn - 2].0.read(&mut buf) {
                    Ok(n) => self.handle_netin(&buf, n, conn - 2),

                    Err(bad) => {
                        writeln!(std::io::stderr(), "bad b! {}", bad).unwrap();
                        event_loop.shutdown();
                    },
                }
            },
        }
    }
}

impl Client {
    // len is the amount of the buffer we actually filled up
    fn handle_netin(&mut self, buf: &Vec<u8>, len: usize, conn: usize) {
        for i in 0..len {
            if buf[i] as char == '\n' { continue }

            if buf[i] as char == '\r' {
                let temp = self.connections[conn].2.clone();
                writeln!(std::io::stderr(), "{}", temp).unwrap();

                self.connections[conn].1.push_front(temp);
                self.connections[conn].2.clear();
            } else {
                self.connections[conn].2.push(buf[i] as char);
            }
        }

        clear();
        draw_scroll(if self.on_ctl { &self.ctl_scroll }
                              else { &self.connections[self.current_conn].1 });
        refresh();
    }

    // len is the amount of the buffer we actually filled up
    fn handle_stdin(&mut self, buf: &Vec<u8>, len: usize) {
        let mut alt = false;
        for i in 0..len {
            writeln!(std::io::stderr(), "key: {:?}", buf[i]).unwrap();
            if alt {
                if buf[i] as char == 'a' {
                    self.on_ctl = !self.on_ctl;
                    clear();
                    draw_scroll(if self.on_ctl { &self.ctl_scroll }
                                          else { &self.connections[self.current_conn].1 });
                    refresh();
                }
                
                if buf[i] >= '0' as u8 && buf[i] <= '9' as u8 {
                    self.current_conn = (buf[i] - '0' as u8) as usize;
                    if self.current_conn >= self.connections.len() {
                        writeln!(std::io::stderr(), "not that many").unwrap();
                        self.current_conn = self.connections.len() - 1;
                    }

                    clear();
                    draw_scroll(if self.on_ctl { &self.ctl_scroll }
                                          else { &self.connections[self.current_conn].1 });
                    refresh();
                }
            } else {
                match buf[i] as char {
                    '\r' => { // this is what return does ?
                        if self.on_ctl {
                            self.ctl_scroll.push_front(self.inbuf.clone());
                            clear();
                            draw_scroll(&self.ctl_scroll);
                        } else {
                            self.connections[self.current_conn].1.push_front(self.inbuf.clone());
                            clear();
                            draw_scroll(&self.connections[self.current_conn].1);

                            self.inbuf.push_str("\r\n");
                            match self.connections[self.current_conn].0.write_all(self.inbuf.as_bytes()) {
                                Ok(_) => (),
                                Err(bad) => println!("bad d! {:?}", bad),
                            }
                        }

                        self.inbuf.clear();
                    },
                    '\x7f' => { // backspace is delete on linux :<
                        self.inbuf.pop();
                    },
                    '\x1b' => {
                        alt = true;
                    }
                    _ => {
                        self.inbuf.push(buf[i] as char);
                    },
                }
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
    event_loop.register_opt(&sock, STDIN,
        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();

    // register foonetic
    let foo_irc = connect("irc.foonetic.net", 6667);
    event_loop.register_opt(&foo_irc, FREENODE,
        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();

    // register freenode
    let free_irc = connect("irc.sushigirl.tokyo", 6667);
    event_loop.register_opt(&free_irc, FOONETIC,
        EventSet::all() ^ EventSet::writable(), PollOpt::empty()).unwrap();

    // Start handling events
    let mut handler = Client { pipe: sock, //foon: irc,
                               inbuf: String::new(), //outbuf: String::new(),
                               //scroll: VecDeque::new(),
                               current_conn: 0,
                               connections: vec![
                                   (free_irc, VecDeque::new(), String::new()),
                                   (foo_irc, VecDeque::new(), String::new()),
                               ],
                               on_ctl: false, ctl_scroll: VecDeque::new() };

    event_loop.run(&mut handler).unwrap();

    // more ncurses bullshit
    endwin();

    println!("we made it to the end");
}
