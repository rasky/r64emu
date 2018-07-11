#![feature(box_syntax)]
#![feature(exact_chunks)]

#[macro_use]
extern crate enum_map;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate static_assertions;

#[macro_use]
extern crate array_macro;

#[macro_use]
extern crate slog;

pub mod bus;
pub mod fp;
pub mod gfx;
pub mod hw;
pub mod int;
pub mod sync;
