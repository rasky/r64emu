#![feature(attr_literals)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

#[macro_use]
extern crate emu_derive;
extern crate byteorder;
extern crate emu;
extern crate pretty_hex;

use emu::bus::be::{Bus, DevPtr, Mem};
use emu::sync;
use pretty_hex::*;
use slog::Drain;
use std::cell::RefCell;
use std::env;
use std::rc::Rc;

mod cartridge;
mod dp;
mod mips64;
mod pi;
mod si;
mod sp;
mod vi;

use cartridge::{Cartridge, CicModel};
use dp::Dp;
use mips64::Cpu;
use pi::Pi;
use si::Si;
use sp::Sp;
use vi::Vi;

#[macro_use]
extern crate error_chain;

mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error) #[cfg(unix)];
        }
    }
}

use errors::*;

#[derive(Default, DeviceBE)]
struct Memory {
    #[mem(size = 4194304, offset = 0x0000_0000, vsize = 0x03F0_0000)]
    rdram: Mem,
}

struct N64 {
    sync: sync::Sync,
    bus: Rc<RefCell<Box<Bus>>>,
    cpu: Rc<RefCell<Box<Cpu>>>,
    cart: DevPtr<Cartridge>,

    mem: DevPtr<Memory>,
    pi: DevPtr<Pi>,
    si: DevPtr<Si>,
    sp: DevPtr<Sp>,
    dp: DevPtr<Dp>,
    vi: DevPtr<Vi>,
}

impl N64 {
    fn new(logger: &slog::Logger, romfn: &str) -> Result<N64> {
        let bus = Rc::new(RefCell::new(Bus::new(logger.new(o!()))));
        let cpu = Rc::new(RefCell::new(Box::new(Cpu::new(
            logger.new(o!()),
            bus.clone(),
        ))));
        let cart = DevPtr::new(Cartridge::new(romfn).chain_err(|| "cannot open rom file")?);
        let mem = DevPtr::new(Memory::default());
        let pi = DevPtr::new(
            Pi::new(logger.new(o!()), bus.clone(), "bios/pifdata.bin")
                .chain_err(|| "cannot open BIOS file")?,
        );
        let sp = DevPtr::new(Sp::new(logger.new(o!())));
        let si = DevPtr::new(Si::new(logger.new(o!())));
        let dp = DevPtr::new(Dp::new(logger.new(o!())));
        let vi = DevPtr::new(Vi::new(logger.new(o!())));

        {
            let mut bus = bus.borrow_mut();
            bus.map_device(0x0000_0000, &mem, 0)?;
            bus.map_device(0x0400_0000, &sp, 1)?;
            bus.map_device(0x0410_0000, &dp, 0)?;
            bus.map_device(0x0404_0000, &sp, 0)?;
            bus.map_device(0x0440_0000, &vi, 0)?;
            bus.map_device(0x0460_0000, &pi, 0)?;
            bus.map_device(0x0480_0000, &si, 0)?;
            bus.map_device(0x1FC0_0000, &pi, 1)?;
            bus.map_device(0x1000_0000, &cart, 0)?;
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

        return Ok(N64 {
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
        });
    }

    // Setup the CIC (copy protection) emulation.
    fn setup_cic(&self) -> Result<()> {
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

    fn run_frame(&mut self) {
        let vi = self.vi.clone();
        self.sync.run_frame(move |evt| match evt {
            sync::Event::HSync(x, y) if x == 0 => {
                vi.borrow_mut().set_line(y);
            }
            _ => panic!("unexpected sync event: {:?}", evt),
        });
    }
}

quick_main!(run);

fn log_build_sync() -> slog::Logger {
    let decorator = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    slog::Logger::root(drain, o!())
}

fn log_build_async() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

fn run() -> Result<()> {
    let logger = log_build_async();
    crit!(logger, "Hello World!");

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        bail!("Usage: r64emu [rom]");
    }

    let mut n64 = N64::new(&logger, &args[1])?;
    n64.setup_cic()?;

    for _ in 0..300 {
        n64.run_frame();
    }

    info!(
        logger,
        "finish";
        o!("pc" => format!("{:x}", n64.cpu.borrow().get_pc()))
    );

    Ok(())
}
