#![feature(nll)]
#![feature(arbitrary_self_types)]
#![feature(test)]
#![feature(associated_type_defaults)]
#![feature(concat_idents)]

extern crate emu;
extern crate num;

#[macro_use]
extern crate slog;

mod arch;
mod cp0;
mod cpu;
mod fpu;
mod traits;

pub(crate) mod decode;
pub(crate) mod mmu;

pub use self::arch::{ArchI, ArchII, ArchIII};
pub use self::cp0::Cp0;
pub use self::cpu::{Cpu, CpuContext, Exception};
pub use self::decode::REG_NAMES;
pub use self::fpu::Fpu;
pub use self::traits::{Arch, Config, Cop, Cop0, CopNull};
