extern crate capnp;
extern crate capnpc;

mod drp_capnp {
    include!("drp_capnp.rs");
}

use drp_capnp::message;

use capnp::serialize_packed;
use capnp::message::{Builder, ReaderOptions};

use std::io::BufRead;
use std::io::stdin;
use std::io::stdout;

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

fn main() {
    //let mut data = Builder::new_default();
    //{
    //    let mut msg = data.init_root::<message::Builder>();

    //    msg.set_source("[Awark");
    //    msg.set_dest("Lan");
    //    msg.set_body("yo");
    //}

    //serialize_packed::write_message(&mut stdout(), &mut data);

    let s = stdin();
    let mut t = s.lock();

    match read_message(&mut t, ReaderOptions::default()) {
        Ok(msg) => print!("{:?}", msg),
        Err(e) => panic!(e),
    }
}
