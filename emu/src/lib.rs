#![feature(box_syntax)]
#![feature(step_trait)]
#![feature(specialization)]

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

#[macro_use]
extern crate imgui;

pub mod bus;
pub mod dbg;
pub mod fp;
pub mod gfx;
pub mod hw;
pub mod int;
pub mod sync;
