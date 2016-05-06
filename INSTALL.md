## Dependencies

DRP currently carries the following package dependencies:

- libsodium-dev\*
- capnproto
- pkg-config\*

\* These packages can only be found in the unstable (sid) debian repository, or
in the repo of any debian unstable derivative (like ubuntu 16.04).

If you would like to build and install these from source, you can find them here:
- https://download.libsodium.org/doc/
- https://capnproto.org/install.html

## Rust

The biggest dependency, though, is Rust. Specifically the Rust nightly build,
as we are currently forced to use some unstable language features. Installation
instructions for rust can be found here (make sure to choose the nightly!):
- https://www.rust-lang.org/downloads.html

## DRP

Once these depencies are installed, the last step is to clone this repository,
cd into it, and run `cargo build` to download all of the rust dependencies and
compile the project.

```
git clone https://github.com/drpchat/drp.git
cd drp
cargo build
```
