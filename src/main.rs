use std::env;

extern crate emu;
use emu::{Bus, Table};

extern crate byteorder;
use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use std::cell::RefCell;
use std::rc::Rc;

mod mips64;

fn main() {
    let args: Vec<String> = env::args().collect();

    let _bus: &Bus<Order = BigEndian> = &Table::<BigEndian>::new();

    let bus = Rc::new(RefCell::new(Box::new(_bus)));

    let mut cpu = mips64::Cpu::new(/*bus.clone()*/);

    let val = bus.borrow_mut().read32(0x12000000);

    cpu.step(args[1].parse::<u32>().unwrap());

    println!("Hello, world! {:x}", val);
}
