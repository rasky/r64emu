#![feature(attr_literals)]

#[macro_use]
extern crate slog;

#[macro_use]
extern crate emu_derive;
extern crate byteorder;
extern crate emu;

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

pub mod cartridge;
pub mod dp;
pub mod mips64;
pub mod pi;
pub mod si;
pub mod sp;
pub mod spvector;
pub mod vi;

mod n64;
pub use n64::N64;
