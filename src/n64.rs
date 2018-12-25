use emu::bus::be::{Bus, DevPtr};
use emu::bus::{CurrentDeviceMap, DeviceGetter, DeviceWithTag};
use emu::dbg;
use emu::dbg::{DebuggerModel, DebuggerRenderer};
use emu::gfx::{GfxBufferMutLE, Rgb888};
use emu::hw;
use emu::sync;
use emu::sync::Subsystem;

use enum_map::Enum;
use slog;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::rc::Rc;

use super::ai::Ai;
use super::cartridge::{Cartridge, CicModel};
use super::dp::Dp;
use super::errors::*;
use super::mi::Mi;
use super::mips64;
use super::pi::Pi;
use super::ri::Ri;
use super::si::Si;
use super::sp::Sp;
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

impl DeviceWithTag for R4300 {
    fn tag() -> &'static str {
        "R4300"
    }
}

pub fn r4300_new(logger: slog::Logger, bus: Rc<RefCell<Box<Bus>>>) -> R4300 {
    R4300 {
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
    }
}

pub struct N64 {
    logger: slog::Logger,
    sync: Box<sync::Sync<SyncEmu>>,
    bus: Rc<RefCell<Box<Bus>>>,
    cart: DevPtr<Cartridge>,

    _mi: DevPtr<Mi>,
    _pi: DevPtr<Pi>,
    _si: DevPtr<Si>,
    sp: DevPtr<Sp>,
    _dp: DevPtr<Dp>,
    vi: DevPtr<Vi>,
    _ai: DevPtr<Ai>,
    _ri: DevPtr<Ri>,
}

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
            _ => None,
        }
    }
}

impl N64 {
    pub fn new(logger: slog::Logger, romfn: &str) -> Result<N64> {
        // N64 timings
        // https://assemblergames.com/threads/mapping-n64-overclockability-achieved-3-0x-multiplier-but-not-3-0x-speed.51656/

        let mut sync = sync::Sync::new(logger.new(o!()), SyncEmu);

        let bus = Rc::new(RefCell::new(Bus::new(sync::Sync::new_logger(&sync))));
        let cpu = Pin::new(Box::new(r4300_new(
            sync::Sync::new_logger(&sync),
            bus.clone(),
        )));
        CurrentDeviceMap().register(cpu);

        let mi = DevPtr::new(Mi::new(sync::Sync::new_logger(&sync)));
        let cart = DevPtr::new(Cartridge::new(romfn).chain_err(|| "cannot open rom file")?);
        let pi = DevPtr::new(
            Pi::new(
                sync::Sync::new_logger(&sync),
                bus.clone(),
                mi.clone(),
                "bios/pifdata.bin",
            )
            .chain_err(|| "cannot open BIOS file")?,
        );
        let dp = DevPtr::new(Dp::new(sync::Sync::new_logger(&sync), bus.clone()));
        let sp = Sp::new(sync::Sync::new_logger(&sync), bus.clone(), &dp, mi.clone())?;
        let si = DevPtr::new(Si::new(sync::Sync::new_logger(&sync)));
        let vi = DevPtr::new(Vi::new(
            sync::Sync::new_logger(&sync),
            bus.clone(),
            mi.clone(),
        ));
        let ai = DevPtr::new(Ai::new(sync::Sync::new_logger(&sync)));
        let ri = DevPtr::new(Ri::new(sync::Sync::new_logger(&sync)));

        {
            // Configure main bus
            let mut bus = bus.borrow_mut();
            bus.map_device(0x0000_0000, &ri, 0)?;
            bus.map_device(0x03F0_0000, &ri, 1)?;
            bus.map_device(0x0400_0000, &sp, 0)?;
            bus.map_device(0x0404_0000, &sp, 1)?;
            bus.map_device(0x0408_0000, &sp, 2)?;
            bus.map_device(0x0410_0000, &dp, 0)?;
            bus.map_device(0x0430_0000, &mi, 0)?;
            bus.map_device(0x0440_0000, &vi, 0)?;
            bus.map_device(0x0450_0000, &ai, 0)?;
            bus.map_device(0x0460_0000, &pi, 0)?;
            bus.map_device(0x0470_0000, &ri, 2)?;
            bus.map_device(0x0480_0000, &si, 0)?;
            bus.map_device(0x1000_0000, &cart, 0)?;
            bus.map_device(0x1800_0000, &cart, 1)?;
            bus.map_device(0x1FC0_0000, &pi, 1)?;
        }

        // Register subsystems into sync
        /*
        {
            sync.register("cpu", cpu.clone(), MAIN_CLOCK + MAIN_CLOCK / 2); // FIXME: uses DIVMOD
            sync.register(
                "sp",
                sp.borrow().core_cpu.as_ref().unwrap().clone(),
                MAIN_CLOCK,
            );
            sync.register("dp", dp.clone().unwrap(), MAIN_CLOCK);
        }
        */

        return Ok(N64 {
            logger,
            sync,
            cart,
            bus,
            _pi: pi,
            _si: si,
            sp: sp,
            _dp: dp,
            vi: vi,
            _ai: ai,
            _ri: ri,
            _mi: mi,
        });
    }

    // Setup the CIC (copy protection) emulation.
    pub fn setup_cic(&self) -> Result<()> {
        // Setup the encryption seed, given the CIC model that we detect
        // by checksumming the ROM header.
        let seed: u32 = match self.cart.borrow().detect_cic_model()? {
            CicModel::Cic6101 => 0x3F, // starfox
            CicModel::Cic6102 => 0x3F, // mario
            CicModel::Cic6103 => 0x78, // banjo
            CicModel::Cic6105 => 0x91, // zelda
            CicModel::Cic6106 => 0x85, // f-zero x
        };
        self.bus.borrow_mut().write::<u32>(0x1FC0_07E4, seed << 8);
        Ok(())
    }
}

impl hw::OutputProducer for N64 {
    fn render_frame(&mut self, screen: &mut GfxBufferMutLE<Rgb888>) {
        let mut vi = self.vi.clone();
        self.sync.run_frame(move |evt| match evt {
            sync::Event::HSync(x, y) if x == 0 => {
                vi.borrow_mut().set_line(y);
            }
            _ => {}
        });

        self.vi.borrow_mut().draw_frame(screen);
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
        let mut vi = self.vi.clone();
        self.sync.trace_frame(
            move |evt| match evt {
                sync::Event::HSync(x, y) if x == 0 => {
                    vi.borrow_mut().set_line(y);
                }
                _ => {}
            },
            tracer,
        )?;

        self.vi.borrow_mut().draw_frame(screen);
        Ok(())
    }

    fn trace_step(&mut self, cpu_name: &str, tracer: &dbg::Tracer) -> dbg::Result<()> {
        match cpu_name {
            MAINCPU_NAME => R4300::get_mut().step(tracer),
            RSPCPU_NAME => {
                // Do not keep sp borrowed by cloning the CPU. This allows
                // RSP to recurse back into the SP (eg: write a SP register) without
                // double-borrows.
                let rsp = self.sp.borrow_mut().core_cpu.as_ref().unwrap().clone();
                return rsp.borrow_mut().step(tracer);
            }
            _ => unreachable!(),
        }
    }

    fn render_debug<'a, 'ui>(&mut self, dr: &DebuggerRenderer<'a, 'ui>) {
        R4300::get_mut().render_debug(dr);
        self.sp
            .borrow_mut()
            .core_cpu
            .as_ref()
            .unwrap()
            .borrow_mut()
            .render_debug(dr);
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
