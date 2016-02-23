extern crate capnp;

pub mod drp_capnp {
    include!(concat!(env!("OUT_DIR"), "/drp_capnp.rs"));
}

pub mod util;

pub use drp_capnp::*;
