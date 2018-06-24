#![feature(attr_literals)]

use std::env;

#[macro_use]
extern crate emu_derive;
extern crate byteorder;
extern crate emu;
extern crate pretty_hex;

use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use emu::bus::be::{Bus, DevPtr, Device, Mem};
use pretty_hex::*;
use std::cell::RefCell;
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
    fn new(romfn: &str) -> Result<N64> {
        let bus = Rc::new(RefCell::new(Bus::new()));
        let cpu = Box::new(Cpu::new(bus.clone()));
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
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        return bail!("Usage: r64emu [rom]");
    }

    let mut n64 = N64::new(&args[1])?;

    n64.cpu.run(10000);

    Ok(())
}
