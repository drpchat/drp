extern crate capnpc;

fn main() {
    ::capnpc::compile("src", &["src/drp.capnp"]).unwrap();
}
