use emu::bus::be::{Bus, DevPtr};
use emu::dbg;
use emu::dbg::{DebuggerModel, DebuggerRenderer, Tracer};
use emu::gfx::{GfxBufferMutLE, Rgb888};
use emu::hw;
use emu::sync;
use slog;
use std::cell::RefCell;
use std::rc::Rc;

use super::ai::Ai;
use super::cartridge::{Cartridge, CicModel};
use super::dp::Dp;
use super::errors::*;
use super::mips64;
use super::pi::Pi;
use super::ri::Ri;
use super::si::Si;
use super::sp::Sp;
use super::vi::Vi;

pub struct N64 {
    logger: slog::Logger,
    sync: Box<sync::Sync>,
    bus: Rc<RefCell<Box<Bus>>>,
    cpu: Rc<RefCell<Box<mips64::Cpu>>>,
    cart: DevPtr<Cartridge>,

    _pi: DevPtr<Pi>,
    _si: DevPtr<Si>,
    sp: DevPtr<Sp>,
    _dp: DevPtr<Dp>,
    vi: DevPtr<Vi>,
    _ai: DevPtr<Ai>,
    _ri: DevPtr<Ri>,
}

impl N64 {
    pub fn new(logger: slog::Logger, romfn: &str) -> Result<N64> {
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

        let mut sync = sync::Sync::new(
            logger.new(o!()),
            sync::Config {
                main_clock: VCLK,
                dot_clock_divider: 4,
                hdots: 773, // 773.5...
                vdots: 525,
                hsyncs: vec![0, 773 / 2], // sync two times per line
                vsyncs: vec![],
            },
        );

        let bus = Rc::new(RefCell::new(Bus::new(sync::Sync::new_logger(&sync))));
        let cpu = Rc::new(RefCell::new(Box::new(mips64::Cpu::new(
            "R4300",
            sync::Sync::new_logger(&sync),
            bus.clone(),
        ))));
        let cart = DevPtr::new(Cartridge::new(romfn).chain_err(|| "cannot open rom file")?);
        let pi = DevPtr::new(
            Pi::new(
                sync::Sync::new_logger(&sync),
                bus.clone(),
                "bios/pifdata.bin",
            )
            .chain_err(|| "cannot open BIOS file")?,
        );
        let sp = Sp::new(sync::Sync::new_logger(&sync), bus.clone())?;
        let si = DevPtr::new(Si::new(sync::Sync::new_logger(&sync)));
        let dp = DevPtr::new(Dp::new(sync::Sync::new_logger(&sync), bus.clone()));
        let vi = DevPtr::new(Vi::new(sync::Sync::new_logger(&sync), bus.clone()));
        let ai = DevPtr::new(Ai::new(sync::Sync::new_logger(&sync)));
        let ri = DevPtr::new(Ri::new(sync::Sync::new_logger(&sync)));

        {
            // Install CPU coprocessors
            //   COP0 -> standard MIPS64 CP0
            //   COP1 -> standard MIPS64 FPU
            let mut cpu = cpu.borrow_mut();
            cpu.set_cop0(mips64::Cp0::new(sync::Sync::new_logger(&sync)));
            cpu.set_cop1(mips64::Fpu::new(sync::Sync::new_logger(&sync)));
        }

        {
            // Configure main bus
            let mut bus = bus.borrow_mut();
            bus.map_device(0x0000_0000, &ri, 0)?;
            bus.map_device(0x03F0_0000, &ri, 1)?;
            bus.map_device(0x0400_0000, &sp, 0)?;
            bus.map_device(0x0404_0000, &sp, 1)?;
            bus.map_device(0x0408_0000, &sp, 2)?;
            bus.map_device(0x0410_0000, &dp, 0)?;
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
        {
            sync.register("cpu", cpu.clone(), MAIN_CLOCK + MAIN_CLOCK / 2); // FIXME: uses DIVMOD
            sync.register("sp", sp.borrow().core_cpu.clone(), MAIN_CLOCK);
            sync.register("dp", dp.clone().unwrap(), MAIN_CLOCK);
        }

        return Ok(N64 {
            logger,
            sync,
            cpu,
            cart,
            bus,
            _pi: pi,
            _si: si,
            sp: sp,
            _dp: dp,
            vi: vi,
            _ai: ai,
            _ri: ri,
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
        self.bus.borrow().write::<u32>(0x1FC0_07E4, seed << 8);
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
        info!(self.logger, "finish"; o!("pc" => format!("{:x}", self.cpu.borrow().ctx().get_pc())));
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

    fn render_debug<'a, 'ui>(&mut self, dr: &DebuggerRenderer<'a, 'ui>) {
        self.cpu.borrow_mut().render_debug(dr);
        self.sp.borrow_mut().core_cpu.borrow_mut().render_debug(dr);
    }
}
