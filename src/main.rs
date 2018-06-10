use std::env;

extern crate emu;
use emu::Bus;

extern crate byteorder;
use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use std::cell::RefCell;
use std::rc::Rc;

mod mips64;

struct Hw {
	rdram: Rc<RefCell<[u8]>>,
}

// fn make_membuf(sz: usize) -> Rc<RefCell<[u8]>> {
// 	let mut v : Vec<u8> = Vec::new();
// 	v.resize(sz, 0);
// 	let mut v2 = v.as_slice();
// 	Rc::new(RefCell::new(*v2))
// }

impl Hw {
	fn new() -> Box<Hw> {
		Box::new(Hw{
			rdram: Rc::new(RefCell::new([0u8; 4*1024*1024])),
		})
	}
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut hw = Hw::new();

    let bus = Rc::new(RefCell::new(Bus::<BigEndian>::new()));

    let mut cpu = mips64::Cpu::new(/*bus.clone()*/);

    bus.borrow_mut().map_mem(0x00000000, 0x03EFFFFF, hw.rdram.clone()).unwrap();

    bus.borrow_mut().write32(0x01000234, 4);
    let val = bus.borrow().read32(0x01000234);
    println!("Hello, world! {:x}", val);

    cpu.step(args[1].parse::<u32>().expect("invalid argument"));

}
