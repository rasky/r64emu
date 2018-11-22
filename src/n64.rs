use emu::bus::be::{Bus, DevPtr, Mem};
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
    sync: sync::Sync,
    bus: Rc<RefCell<Box<Bus>>>,
    cpu: Rc<RefCell<Box<mips64::Cpu>>>,
    cart: DevPtr<Cartridge>,

    pi: DevPtr<Pi>,
    si: DevPtr<Si>,
    sp: DevPtr<Sp>,
    dp: DevPtr<Dp>,
    vi: DevPtr<Vi>,
    ai: DevPtr<Ai>,
    ri: DevPtr<Ri>,
}

impl N64 {
    pub fn new(logger: slog::Logger, romfn: &str) -> Result<N64> {
        let bus = Rc::new(RefCell::new(Bus::new(logger.new(o!()))));
        let cpu = Rc::new(RefCell::new(Box::new(mips64::Cpu::new(
            logger.new(o!()),
            bus.clone(),
        ))));
        let cart = DevPtr::new(Cartridge::new(romfn).chain_err(|| "cannot open rom file")?);
        let pi = DevPtr::new(
            Pi::new(logger.new(o!()), bus.clone(), "bios/pifdata.bin")
                .chain_err(|| "cannot open BIOS file")?,
        );
        let sp = Sp::new(logger.new(o!()), bus.clone())?;
        let si = DevPtr::new(Si::new(logger.new(o!())));
        let dp = DevPtr::new(Dp::new(logger.new(o!()), bus.clone()));
        let vi = DevPtr::new(Vi::new(logger.new(o!()), bus.clone()));
        let ai = DevPtr::new(Ai::new(logger.new(o!())));
        let ri = DevPtr::new(Ri::new(logger.new(o!())));

        {
            // Install CPU coprocessors
            //   COP0 -> standard MIPS64 CP0
            //   COP1 -> standard MIPS64 FPU
            let mut cpu = cpu.borrow_mut();
            cpu.set_cop0(mips64::Cp0::new(logger.new(o!())));
            cpu.set_cop1(mips64::Fpu::new(logger.new(o!())));
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

        // N64 timings
        // https://assemblergames.com/threads/mapping-n64-overclockability-achieved-3-0x-multiplier-but-not-3-0x-speed.51656/

        // Oscillators
        const X1: i64 = 14_705_000;
        const X2: i64 = 14_318_000;

        const RDRAM_CLOCK: i64 = X1 * 17;
        const MAIN_CLOCK: i64 = RDRAM_CLOCK / 4;
        const PIF_CLOCK: i64 = MAIN_CLOCK / 4;
        const CARTRIDGE_CLOCK: i64 = PIF_CLOCK / 8; // 1.953 MHZ
        const VCLK: i64 = X2 * 17 / 5; // 48.6812 MHZ

        let mut sync = sync::Sync::new(sync::Config {
            main_clock: VCLK,
            dot_clock_divider: 4,
            hdots: 773, // 773.5...
            vdots: 263,
            hsyncs: vec![0], // sync at the beginning of each line
            vsyncs: vec![],
        });
        sync.register(cpu.clone(), MAIN_CLOCK + MAIN_CLOCK / 2); // FIXME: uses DIVMOD
        sync.register(sp.borrow().core_cpu.clone(), MAIN_CLOCK);
        sync.register(dp.clone().unwrap(), MAIN_CLOCK);

        return Ok(N64 {
            logger,
            sync,
            cpu,
            cart,
            bus,
            pi,
            si,
            sp,
            dp,
            vi,
            ai,
            ri,
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
            _ => panic!("unexpected sync event: {:?}", evt),
        });

        self.vi.borrow().draw_frame(screen);
    }

    fn finish(&mut self) {
        info!(self.logger, "finish"; o!("pc" => format!("{:x}", self.cpu.borrow().ctx().get_pc())));
    }
}
