#![feature(box_syntax)]

#[macro_use]
extern crate enum_map;

mod bus;
mod regs;

pub use self::bus::Bus;
