## Installation
DRP depends on the following packages:
- libsodium-dev
- pkg-config
- capnproto

and the Rust **Nightly** build found here:
 
https://www.rust-lang.org/downloads.html

### Building
Once you have the appropriate dependencies, clone this repository and build the
project.

```
git clone https://github.com/drpchat/drp.git

cd drp

cargo build
```

## Usage
Once you have built the client binary, you can either run it with `cargo run
--bin drpc <nick> <server>` or by navigating to `drp/target/debug/` where the
`drpc` binary is located.

With the client running, you have the following commands availible to you:
- `/m <nick>` or `/msg <nick>` : sends a message to a user
- `/w <nick>` or `/whois <nick>` : asks the server to retrieve the public key
  of a user so that messages can be sent encrypted
- `/j <channel>` or `/join <channel>` : joins or creates and joins a channel
- `/p <channel>` or `/part <channel>` : leaves a channel that you are in

Importantly, if you want to send an encrypted communication you will need to
`/whois` the target user first to exchange keys, otherwise messages will be
sent in plaintext.
