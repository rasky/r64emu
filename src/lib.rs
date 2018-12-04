#![feature(nll)]
#![feature(stdsimd)]

#[macro_use]
extern crate slog;

#[macro_use]
extern crate emu_derive;
extern crate byteorder;
#[macro_use]
extern crate emu;
extern crate mips64;

extern crate packed_simd;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate error_chain;

pub mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error) #[cfg(unix)];
        }
    }
}

mod rdp;

pub mod ai;
pub mod cartridge;
pub mod dp;
pub mod pi;
pub mod ri;
pub mod si;
pub mod sp;
pub mod vi;

mod n64;
pub use n64::N64;
