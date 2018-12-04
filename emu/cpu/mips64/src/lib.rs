#![feature(nll)]
#![feature(arbitrary_self_types)]

extern crate emu;
extern crate num;

#[macro_use]
extern crate slog;

mod cp0;
mod cpu;
mod fpu;

pub(crate) mod decode;

pub use self::cp0::Cp0;
pub use self::cpu::{Cop, Cop0, Cpu, CpuContext, Exception};
pub use self::decode::{DecodedInsn, REG_NAMES};
pub use self::fpu::Fpu;
