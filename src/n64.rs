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
use super::si::Si;
use super::sp::Sp;
use super::vi::Vi;

#[derive(Default, DeviceBE)]
struct Memory {
    #[mem(size = 4194304, offset = 0x0000_0000, vsize = 0x03F0_0000)]
    rdram: Mem,
}

pub struct N64 {
    logger: slog::Logger,
    sync: sync::Sync,
    bus: Rc<RefCell<Box<Bus>>>,
    cpu: Rc<RefCell<Box<mips64::Cpu>>>,
    cart: DevPtr<Cartridge>,

    mem: DevPtr<Memory>,
    pi: DevPtr<Pi>,
    si: DevPtr<Si>,
    sp: DevPtr<Sp>,
    dp: DevPtr<Dp>,
    vi: DevPtr<Vi>,
    ai: DevPtr<Ai>,
}

impl N64 {
    pub fn new(logger: slog::Logger, romfn: &str) -> Result<N64> {
        let bus = Rc::new(RefCell::new(Bus::new(logger.new(o!()))));
        let cpu = Rc::new(RefCell::new(Box::new(mips64::Cpu::new(
            logger.new(o!()),
            bus.clone(),
        ))));
        let cart = DevPtr::new(Cartridge::new(romfn).chain_err(|| "cannot open rom file")?);
        let mem = DevPtr::new(Memory::default());
        let pi = DevPtr::new(
            Pi::new(logger.new(o!()), bus.clone(), "bios/pifdata.bin")
                .chain_err(|| "cannot open BIOS file")?,
        );
        let sp = Sp::new(logger.new(o!()), bus.clone())?;
        let si = DevPtr::new(Si::new(logger.new(o!())));
        let dp = DevPtr::new(Dp::new(logger.new(o!()), bus.clone()));
        let vi = DevPtr::new(Vi::new(logger.new(o!()), bus.clone()));
        let ai = DevPtr::new(Ai::new(logger.new(o!())));

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
            bus.map_device(0x0000_0000, &mem, 0)?;
            bus.map_device(0x0400_0000, &sp, 0)?;
            bus.map_device(0x0404_0000, &sp, 1)?;
            bus.map_device(0x0408_0000, &sp, 2)?;
            bus.map_device(0x0410_0000, &dp, 0)?;
            bus.map_device(0x0440_0000, &vi, 0)?;
            bus.map_device(0x0450_0000, &ai, 0)?;
            bus.map_device(0x0460_0000, &pi, 0)?;
            bus.map_device(0x0480_0000, &si, 0)?;
            bus.map_device(0x1000_0000, &cart, 0)?;
            bus.map_device(0x1FC0_0000, &pi, 1)?;
        }

        const MAIN_CLOCK: i64 = 187488000; // TODO: guessed

        let mut sync = sync::Sync::new(sync::Config {
            main_clock: MAIN_CLOCK,
            dot_clock_divider: 8,
            hdots: 744,
            vdots: 525,
            hsyncs: vec![0], // sync at the beginning of each line
            vsyncs: vec![],
        });
        sync.register(cpu.clone(), MAIN_CLOCK / 2);
        sync.register(sp.borrow().core_cpu.clone(), MAIN_CLOCK / 3);
        sync.register(dp.clone().unwrap(), MAIN_CLOCK / 3);

        return Ok(N64 {
            logger,
            sync,
            cpu,
            cart,
            bus,
            mem,
            pi,
            si,
            sp,
            dp,
            vi,
            ai,
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
