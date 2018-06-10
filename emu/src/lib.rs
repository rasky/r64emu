#![feature(box_syntax)]

#[macro_use]
extern crate enum_map;

#[macro_use]
extern crate static_assertions;

mod bus;
mod regs;

pub use self::bus::Bus;
