extern crate sodiumoxide;
extern crate rustc_serialize;

use sodiumoxide::crypto::box_;
use rustc_serialize::hex::*;
use std::io::*;
use sodiumoxide::crypto::box_::curve25519xsalsa20poly1305::PublicKey;
use std::fs::File;
//use std::io::BufReader;
//use std::io::BufRead;


fn main() {
    let (mut ourpk, mut oursk) = box_::gen_keypair();
    let hexy = ourpk.0.to_hex();
    println!("Public Key: {}", hexy);
    let mut f = File::create("./pk.key").unwrap();
    f.write_all(hexy.as_bytes()).unwrap();
    let hexy = oursk.0.to_hex();
    println!("Secret Key: {}", hexy);
    let mut f = File::create("./sk.key").unwrap();
    f.write_all(hexy.as_bytes()).unwrap();
    let (theirpk, theirsk) = box_::gen_keypair();
    let our_precomputed_key = box_::precompute(&theirpk, &oursk);
    let nonce = box_::gen_nonce();
    let plaintext = b"plaintext";
    let ciphertext = box_::seal_precomputed(plaintext, &nonce, &our_precomputed_key);
    // this will be identical to our_precomputed_key
    let their_precomputed_key = box_::precompute(&ourpk, &theirsk);
    let their_plaintext = box_::open_precomputed(&ciphertext, &nonce,
                                             &their_precomputed_key).unwrap();
    assert!(plaintext == &their_plaintext[..]);
}
