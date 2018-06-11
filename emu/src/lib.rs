#![feature(box_syntax)]

#[macro_use]
extern crate enum_map;

#[macro_use]
extern crate static_assertions;

pub mod bus;
mod regs;
mod memint;
