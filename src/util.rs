use capnp::message::{Builder, HeapAllocator};

use drp_capnp::message;

// todo: use bytes instead of string
#[derive(Debug)]
pub enum Message {
    Register { name: Vec<u8> },
    Send { dest: Vec<u8>, body: Vec<u8> },
    Relay { source: Vec<u8>, dest: Vec<u8>, body: Vec<u8>, },
}

pub fn serialize<A>(msg: Message) -> Builder<HeapAllocator> {
    match msg {
        Message::Register { name } => serialize_register(name.as_slice()),
        Message::Send { dest , body } =>
            serialize_send(dest.as_slice(), body.as_slice()),
        Message::Relay { source, dest, body } =>
            serialize_relay(source.as_slice(),
                dest.as_slice(), body.as_slice()),
    }
}

pub fn serialize_register(name: &[u8]) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    data.init_root::<message::register::Builder>().set_name(name);
    data
}

pub fn serialize_send(dest: &[u8], body: &[u8]) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_send();

        mm.set_dest(dest);
        mm.set_body(body);
    }
    data
}

pub fn serialize_relay(source: &[u8], dest: &[u8], body: &[u8]) -> Builder<HeapAllocator> {
    let mut data = Builder::new_default();
    {
        let msg = data.init_root::<message::Builder>();
        let mut mm = msg.init_relay();

        mm.set_source(source);
        mm.set_dest(dest);
        mm.set_body(body);
    }
    data
}
