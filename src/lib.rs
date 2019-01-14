#![feature(nll)]
#![feature(stdsimd)]
#![feature(pin)]

#[macro_use]
extern crate slog;

#[macro_use]
extern crate emu_derive;
extern crate byteorder;
extern crate emu;
extern crate mips64;

extern crate packed_simd;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate error_chain;

pub mod errors {
    error_chain! {
        foreign_links {
            Io(::std::io::Error);
        }
    }
}

mod rdp;

pub mod ai;
pub mod r4300;
pub mod cartridge;
pub mod dp;
pub mod mi;
pub mod pi;
pub mod ri;
pub mod si;
pub mod sp;
pub mod vi;

mod n64;
pub use self::n64::N64;
