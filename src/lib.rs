#![feature(attr_literals)]
#![feature(stdsimd)]

#[macro_use]
extern crate slog;

#[macro_use]
extern crate emu_derive;
extern crate byteorder;
extern crate emu;

extern crate packed_simd;

#[macro_use]
extern crate bitflags;

extern crate bit_field;

#[macro_use]
extern crate bitfield;

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
mod vops;

pub mod ai;
pub mod cartridge;
pub mod dp;
pub mod mips64;
pub mod pi;
pub mod ri;
pub mod si;
pub mod sp;
pub mod spvector;
pub mod vi;

mod n64;
pub use n64::N64;
