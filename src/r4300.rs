use std::ops::{Deref, DerefMut};
use mips64;
use emu::bus::be::{Bus, Device};

use super::n64::MAINCPU_NAME;
use super::ai::Ai;
use super::cartridge::{Cartridge, CicModel};
use super::dp::Dp;
use super::errors::*;
use super::mi::Mi;
use super::pi::Pi;
use super::ri::Ri;
use super::si::Si;
use super::sp::{Sp, RSPCPU};
use super::vi::Vi;

pub struct R4300Config;

impl mips64::Config for R4300Config {
    type Arch = mips64::ArchIII; // 64-bit MIPS III architecture
    type Cop0 = mips64::Cp0;
    type Cop1 = mips64::Fpu;
    type Cop2 = mips64::CopNull;
    type Cop3 = mips64::CopNull;
}

#[derive(DeviceBE)]
pub struct R4300 {
    cpu: mips64::Cpu<R4300Config>,
}

impl Deref for R4300 {
    type Target = mips64::Cpu<R4300Config>;
    fn deref(&self) -> &Self::Target {
        &self.cpu
    }
}

impl DerefMut for R4300 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cpu
    }
}

impl R4300 {
    pub fn new(logger: slog::Logger) -> Box<Self> {
        Box::new(R4300 {
            cpu: mips64::Cpu::new(
                MAINCPU_NAME,
                logger.new(o!()),
                Bus::new(logger.new(o!())),
                (
                    mips64::Cp0::new("R4300-COP0", logger.new(o!())),
                    mips64::Fpu::new("R4300-FPU", logger.new(o!())),
                    mips64::CopNull {},
                    mips64::CopNull {},
                ),
            ),
        })
    }

    pub fn map_bus(&mut self) -> Result<()> {
        self.bus.map_device(0x0000_0000, Ri::get(), 0)?;
        self.bus.map_device(0x03F0_0000, Ri::get(), 1)?;
        self.bus.map_device(0x0400_0000, Sp::get(), 0)?;
        self.bus.map_device(0x0404_0000, Sp::get(), 1)?;
        self.bus.map_device(0x0408_0000, Sp::get(), 2)?;
        self.bus.map_device(0x0410_0000, Dp::get(), 0)?;
        self.bus.map_device(0x0430_0000, Mi::get(), 0)?;
        self.bus.map_device(0x0440_0000, Vi::get(), 0)?;
        self.bus.map_device(0x0450_0000, Ai::get(), 0)?;
        self.bus.map_device(0x0460_0000, Pi::get(), 0)?;
        self.bus.map_device(0x0470_0000, Ri::get(), 2)?;
        self.bus.map_device(0x0480_0000, Si::get(), 0)?;
        self.bus.map_device(0x1000_0000, Cartridge::get(), 0)?;
        self.bus.map_device(0x1800_0000, Cartridge::get(), 1)?;
        self.bus.map_device(0x1FC0_0000, Pi::get(), 1)?;
        Ok(())
    }
}