use std::env;

extern crate emu;
use emu::Bus;

extern crate byteorder;
use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use std::cell::RefCell;
use std::rc::Rc;

mod mips64;

fn main() {
    let args: Vec<String> = env::args().collect();

    let bus = Rc::new(RefCell::new(Bus::<BigEndian>::new()));

    let mut cpu = mips64::Cpu::new(/*bus.clone()*/);

    let mut val = bus.borrow().read32(0x12000000);
    val += bus.borrow().read32(0x12000004);

    let mut x = bus.borrow_mut();
    x.write32(0x12000000, 4);
    x.write32(0x12000000, 6);

    cpu.step(args[1].parse::<u32>().unwrap());

    println!("Hello, world! {:x}", val);
}
