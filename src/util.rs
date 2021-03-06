#![macro_use]

use capnp::message::{Builder, HeapAllocator};
use capnp::message::{Reader, ReaderSegments};
use capnp::{Error, Result};

use drp_capnp::message;

#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => ((writeln!(&mut ::std::io::stderr(), $($arg)*).unwrap()))
}

// todo: use bytes instead of string
#[derive(Debug)]
pub enum Message<'a> {
    Register { name: &'a str, pubkey: &'a [u8] },
    Send { dest: &'a str, body: &'a [u8], nonce: Option<&'a [u8]> },
    Relay { source: &'a str, dest: &'a str, body: &'a [u8],
        nonce: Option<&'a [u8]> },
    Join { channel: &'a str },
    Part { channel: &'a str },
    Response { body: &'a str },
    Whois { name: &'a str },
    Theyare { name: &'a str, pubkey: &'a [u8] },
}

pub fn serialize<A>(msg: Message) -> Builder<HeapAllocator> {
    match msg {
        Message::Register { name, pubkey } => serialize_register(name, pubkey),
        Message::Send { dest , body, nonce } =>
            serialize_send(dest, body, nonce),
        Message::Relay { source, dest, body, nonce } =>
            serialize_relay(source, dest, body, nonce),
        Message::Join { channel } => serialize_join(channel),
        Message::Part { channel } => serialize_part(channel),
        Message::Response { body } => serialize_response(body),
        Message::Whois { name } => serialize_whois(name),
        Message::Theyare { name, pubkey } => serialize_theyare(name, pubkey),
    }
}

pub fn serialize_register(name: &str, pubkey: &[u8]) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_register();
        mm.set_name(name);
        mm.set_pubkey(pubkey);
    }
    data
}

pub fn serialize_send(dest: &str, body: &[u8], nonce: Option<&[u8]>)
    -> Builder<HeapAllocator> {

    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_send();

        mm.set_dest(dest);
        mm.set_body(body);
        match nonce {
            None => mm.set_unencrypted(()),
            Some(nonce) => mm.set_nonce(nonce),
        }
    }
    data
}

pub fn serialize_relay(source: &str, dest: &str, body: &[u8],
    nonce: Option<&[u8]>) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_relay();

        mm.set_source(source);
        mm.set_dest(dest);
        mm.set_body(body);
        match nonce {
            Some(nonce) => mm.set_nonce(nonce),
            None => mm.set_unencrypted(()),
        }
    }
    data
}

pub fn serialize_join(channel: &str) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_join();

        mm.set_channel(channel);
    }
    data
}

pub fn serialize_part(channel: &str) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_part();

        mm.set_channel(channel);
    }
    data
}

pub fn serialize_response(body: &str) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_response();

        mm.set_body(body);
    }
    data
}

pub fn serialize_whois(name: &str) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_whois();

        mm.set_name(name);
    }
    data
}

pub fn serialize_theyare(name: &str, pubkey: &[u8]) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_theyare();

        mm.set_name(name);
        mm.set_pubkey(pubkey);
    }
    data
}

pub fn deserialize<'a, S>(msg: &'a Reader<S>) -> Result<Message<'a>>
    where S: ReaderSegments {

    let msg = try!(msg.get_root::<message::Reader>());

    match msg.which() {
        Ok(msg) => match msg {
            message::Register(msg) => deserialize_register(msg),
            message::Send(msg) => deserialize_send(msg),
            message::Relay(msg) => deserialize_relay(msg),
            message::Join(msg) => deserialize_join(msg),
            message::Part(msg) => deserialize_part(msg),
            message::Response(msg) => deserialize_response(msg),
            message::Whois(msg) => deserialize_whois(msg),
            message::Theyare(msg) => deserialize_theyare(msg),
        },
        Err(e) => Err(Error::from(e)),
    }
}

pub fn deserialize_register<'a>(msg: message::register::Reader<'a>)
    -> Result<Message<'a>> {

    Ok(Message::Register {
        name: try!(msg.get_name()),
        pubkey: try!(msg.get_pubkey()),
    })
}

pub fn deserialize_send<'a>(msg: message::send::Reader<'a>)
    -> Result<Message<'a>> {

    Ok(Message::Send {
        dest: try!(msg.get_dest()),
        body: try!(msg.get_body()),

        nonce: try!(match msg.which() {
            Ok(message::send::Nonce(msg)) => msg.map(Some),
            Ok(message::send::Unencrypted(())) => Ok(None),
            Err(e) => Err(Error::from(e)),
        }),
    })
}

pub fn deserialize_relay<'a>(msg: message::relay::Reader<'a>)
    -> Result<Message<'a>> {

    Ok(Message::Relay {
        source: try!(msg.get_source()),
        dest: try!(msg.get_dest()),
        body: try!(msg.get_body()),

        nonce: try!(match msg.which() {
            Ok(message::relay::Nonce(msg)) => msg.map(Some),
            Ok(message::relay::Unencrypted(())) => Ok(None),
            Err(e) => Err(Error::from(e)),
        }),
    })
}

pub fn deserialize_join<'a>(msg: message::join::Reader<'a>)
    -> Result<Message<'a>> {

    Ok(Message::Join { channel: try!(msg.get_channel()) })
}

pub fn deserialize_part<'a>(msg: message::part::Reader<'a>)
    -> Result<Message<'a>> {
    Ok(Message::Part { channel: try!(msg.get_channel()) })
}

pub fn deserialize_response<'a>(msg: message::response::Reader<'a>)
    -> Result<Message<'a>> {
    Ok(Message::Response { body: try!(msg.get_body()) })
}

pub fn deserialize_whois<'a>(msg: message::whois::Reader<'a>)
    -> Result<Message<'a>> {

    Ok(Message::Whois { name: try!(msg.get_name()) })
}

pub fn deserialize_theyare<'a>(msg: message::theyare::Reader<'a>)
    -> Result<Message<'a>> {

    Ok(Message::Theyare {
        name: try!(msg.get_name()),
        pubkey: try!(msg.get_pubkey()),
    })
}
