use emu::bus::be::{Bus, Device};
use emu::dbg;
use emu::dbg::{DebuggerModel, DebuggerRenderer};
use emu::gfx::{GfxBufferMutLE, Rgb888};
use emu::hw;
use emu::sync;
use emu::sync::Subsystem;
use emu_derive::DeviceBE;

use slog;
use std::ops::{Deref, DerefMut};

use super::ai::Ai;
use super::cartridge::{Cartridge, CicModel};
use super::dp::Dp;
use super::errors::*;
use super::mi::Mi;
use super::mips64;
use super::pi::Pi;
use super::ri::Ri;
use super::si::Si;
use super::sp::{Sp, RSPCPU};
use super::vi::Vi;

// Used in debugger windows
const MAINCPU_NAME: &'static str = "R4300";
const RSPCPU_NAME: &'static str = "RSP";

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
    pub fn new(logger: slog::Logger, bus: Box<Bus>) -> Box<Self> {
        Box::new(R4300 {
            cpu: mips64::Cpu::new(
                MAINCPU_NAME,
                logger.new(o!()),
                bus,
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

pub struct N64 {
    logger: slog::Logger,
    sync: Box<sync::Sync<SyncEmu>>,
}

// N64 timings
// https://assemblergames.com/threads/mapping-n64-overclockability-achieved-3-0x-multiplier-but-not-3-0x-speed.51656/

// Oscillators
const X1: i64 = 14_705_000;
const X2: i64 = 14_318_000;

const RDRAM_CLOCK: i64 = X1 * 17;
const MAIN_CLOCK: i64 = RDRAM_CLOCK / 4;
const _PIF_CLOCK: i64 = MAIN_CLOCK / 4;
const _CARTRIDGE_CLOCK: i64 = _PIF_CLOCK / 8; // 1.953 MHZ
const VCLK: i64 = X2 * 17 / 5; // 48.6812 MHZ

struct SyncEmu;
impl sync::SyncEmu for SyncEmu {
    fn config(&self) -> sync::Config {
        sync::Config {
            main_clock: VCLK,
            dot_clock_divider: 4,
            hdots: 773, // 773.5...
            vdots: 525,
            hsyncs: vec![0, 773 / 2], // sync two times per line
            vsyncs: vec![],
        }
    }
    fn subsystem(&self, idx: usize) -> Option<(&mut dyn sync::Subsystem, i64)> {
        match idx {
            0 => Some((R4300::get_mut().deref_mut(), MAIN_CLOCK + MAIN_CLOCK / 2)), // FIXME: uses DIVMOD),
            1 => Some((RSPCPU::get_mut().deref_mut(), MAIN_CLOCK)),
            2 => Some((Dp::get_mut(), MAIN_CLOCK)),
            _ => None,
        }
    }
}

impl N64 {
    pub fn new(logger: slog::Logger, romfn: &str) -> Result<N64> {
        let sync = sync::Sync::new(logger.new(o!()), SyncEmu);

        let bus = Bus::new(sync::Sync::new_logger(&sync));

        R4300::new(sync::Sync::new_logger(&sync), bus).register();
        Mi::new(sync::Sync::new_logger(&sync)).register();
        Cartridge::new(romfn)
            .chain_err(|| "cannot open rom file")?
            .register();

        Pi::new(sync::Sync::new_logger(&sync), "bios/pifdata.bin")
            .chain_err(|| "cannot open BIOS file")?
            .register();
        Dp::new(sync::Sync::new_logger(&sync)).register();
        Sp::new(sync::Sync::new_logger(&sync))?.register();
        Si::new(sync::Sync::new_logger(&sync)).register();
        Vi::new(sync::Sync::new_logger(&sync)).register();
        Ai::new(sync::Sync::new_logger(&sync)).register();
        Ri::new(sync::Sync::new_logger(&sync)).register();

        // Now that all devices have been created, map the CPU buses.
        R4300::get_mut().map_bus()?;
        RSPCPU::get_mut().map_bus()?;

        return Ok(N64 { logger, sync });
    }

    // Setup the CIC (copy protection) emulation.
    pub fn setup_cic(&mut self) -> Result<()> {
        // Setup the encryption seed, given the CIC model that we detect
        // by checksumming the ROM header.
        let seed: u32 = match Cartridge::get().detect_cic_model()? {
            CicModel::Cic6101 => 0x3F, // starfox
            CicModel::Cic6102 => 0x3F, // mario
            CicModel::Cic6103 => 0x78, // banjo
            CicModel::Cic6105 => 0x91, // zelda
            CicModel::Cic6106 => 0x85, // f-zero x
        };
        R4300::get_mut().bus.write::<u32>(0x1FC0_07E4, seed << 8);
        Ok(())
    }
}

impl hw::OutputProducer for N64 {
    fn render_frame(&mut self, screen: &mut GfxBufferMutLE<Rgb888>) {
        self.sync.run_frame(move |evt| match evt {
            sync::Event::HSync(x, y) if x == 0 => {
                Vi::get_mut().set_line(y);
            }
            _ => {}
        });

        Vi::get_mut().draw_frame(screen);
    }

    fn finish(&mut self) {
        info!(self.logger, "finish"; o!("pc" => format!("{:x}", R4300::get().ctx().get_pc())));
    }
}

impl DebuggerModel for N64 {
    fn trace_frame(
        &mut self,
        screen: &mut GfxBufferMutLE<Rgb888>,
        tracer: &dbg::Tracer,
    ) -> dbg::Result<()> {
        self.sync.trace_frame(
            move |evt| match evt {
                sync::Event::HSync(x, y) if x == 0 => {
                    Vi::get_mut().set_line(y);
                }
                _ => {}
            },
            tracer,
        )?;

        Vi::get_mut().draw_frame(screen);
        Ok(())
    }

    fn trace_step(&mut self, cpu_name: &str, tracer: &dbg::Tracer) -> dbg::Result<()> {
        match cpu_name {
            MAINCPU_NAME => R4300::get_mut().step(tracer),
            RSPCPU_NAME => RSPCPU::get_mut().step(tracer),
            _ => unreachable!(),
        }
    }

    fn render_debug<'a, 'ui>(&mut self, dr: &DebuggerRenderer<'a, 'ui>) {
        R4300::get_mut().render_debug(dr);
        RSPCPU::get_mut().render_debug(dr);
    }

    fn all_cpus(&self) -> Vec<String> {
        vec![MAINCPU_NAME.into(), RSPCPU_NAME.into()]
    }

    fn cycles(&self) -> i64 {
        self.sync.cycles()
    }

    fn frames(&self) -> i64 {
        self.sync.frames()
    }
}
