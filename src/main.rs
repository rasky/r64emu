#![feature(attr_literals)]

#[macro_use]
extern crate slog;
extern crate sloggers;

#[macro_use]
extern crate emu_derive;
extern crate byteorder;
extern crate emu;
extern crate pretty_hex;

use emu::bus::be::{Bus, DevPtr, Mem};
use pretty_hex::*;
use sloggers::terminal::{Destination, TerminalLoggerBuilder};
use sloggers::types::Severity;
use sloggers::Build;
use std::cell::RefCell;
use std::env;
use std::rc::Rc;

mod cartridge;
mod mips64;
mod pi;

use cartridge::Cartridge;
use mips64::Cpu;
use pi::Pi;

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
    bus: Rc<RefCell<Box<Bus>>>,
    cpu: Box<Cpu>,
    cart: DevPtr<Cartridge>,

    mem: DevPtr<Memory>,
    pi: DevPtr<Pi>,
}

impl N64 {
    fn new(logger: &slog::Logger, romfn: &str) -> Result<N64> {
        let bus = Rc::new(RefCell::new(Bus::new()));
        let cpu = Box::new(Cpu::new(logger.new(o!()), bus.clone()));
        let cart = DevPtr::new(Cartridge::new(romfn).chain_err(|| "cannot open rom file")?);
        let mem = DevPtr::new(Memory::default());
        let pi = DevPtr::new(Pi::new("bios/pifdata.bin").chain_err(|| "cannot open BIOS file")?);

        {
            let mut bus = bus.borrow_mut();
            bus.map_device(0x0000_0000, &mem, 0)?;
            bus.map_device(0x0460_0000, &pi, 0)?;
            bus.map_device(0x1FC0_0000, &pi, 1)?;
            bus.map_device(0x1000_0000, &cart, 0)?;
        }

        return Ok(N64 {
            cpu,
            cart,
            bus,
            mem,
            pi,
        });
    }
}

quick_main!(run);

fn run() -> Result<()> {
    let mut builder = TerminalLoggerBuilder::new();
    builder.level(Severity::Debug);
    builder.destination(Destination::Stderr);

    let logger = builder.build().unwrap();
    crit!(logger, "Hello World!");

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        bail!("Usage: r64emu [rom]");
    }

    let mut n64 = N64::new(&logger, &args[1])?;

    n64.cpu.run(10000);

    Ok(())
}
