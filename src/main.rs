use std::env;

extern crate emu;
use emu::bus::Bus;

extern crate byteorder;
use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use std::cell::RefCell;
use std::rc::Rc;

mod mips64;

struct Hw {
    rdram: RefCell<[u8; 4 * 1024 * 1024]>,
}

// fn make_membuf(sz: usize) -> Rc<RefCell<[u8]>> {
//  let mut v : Vec<u8> = Vec::new();
//  v.resize(sz, 0);
//  let mut v2 = v.as_slice();
//  Rc::new(RefCell::new(*v2))
// }

impl Hw {
    fn new() -> Box<Hw> {
        Box::new(Hw {
            rdram: RefCell::new([0u8; 4 * 1024 * 1024]),
        })
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let hw = Hw::new();

    let bus = Rc::new(RefCell::new(Bus::<BigEndian>::new()));

    let mut cpu = mips64::Cpu::new(/*bus.clone()*/);

    bus.borrow_mut()
        .map_mem(0x00000000, 0x03EFFFFF, &hw.rdram)
        .unwrap();

    bus.borrow_mut().write::<u32>(0x01000234, 4);
    let val1 = bus.borrow().read::<u16>(0x01000234);
    let val2 = bus.borrow().read::<u16>(0x01000234 + 2);
    let val3 = bus.borrow().fetch_read::<u16>(0x01000234 + 2).read();
    println!("Hello, world! {:x} / {:x} : {:x}", val1, val2, val3);

    cpu.step(args[1].parse::<u32>().expect("invalid argument"));
}
