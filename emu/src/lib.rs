#![feature(box_syntax)]

#[macro_use]
extern crate enum_map;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate static_assertions;

#[macro_use]
extern crate array_macro;

#[allow(unused_imports)]
#[macro_use]
extern crate emu_derive;

pub mod bus;
pub use emu_derive::*;
