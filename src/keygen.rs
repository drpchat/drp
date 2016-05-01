extern crate sodiumoxide;
extern crate rustc_serialize;

use sodiumoxide::crypto::box_;
use rustc_serialize::hex::*;
use std::io::*;
use std::fs::File;

fn main() {
    let (ourpk, oursk) = box_::gen_keypair();
    let hexy = ourpk.0.to_hex();
    println!("Public Key: {}", hexy);
    let mut f = File::create("./pk.key").unwrap();
    f.write_all(hexy.as_bytes()).unwrap();
    let hexy = oursk.0.to_hex();
    println!("Secret Key: {}", hexy);
    let mut f = File::create("./sk.key").unwrap();
    f.write_all(hexy.as_bytes()).unwrap();
}
