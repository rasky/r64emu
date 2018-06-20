use std::env;

extern crate emu;
use emu::bus::be::{Bus, Mem, MemFlags};

extern crate byteorder;
use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use std::cell::RefCell;
use std::rc::Rc;

mod mips64;

struct Hw<'a> {
    rdram: Mem,
    bus: Box<Bus<'a>>,
}

// fn make_membuf(sz: usize) -> Rc<RefCell<[u8]>> {
//  let mut v : Vec<u8> = Vec::new();
//  v.resize(sz, 0);
//  let mut v2 = v.as_slice();
//  Rc::new(RefCell::new(*v2))
// }

impl<'a> Hw<'a> {
    fn new() -> Box<Hw<'a>> {
        Box::new(Hw {
            rdram: Mem::new(4 * 1024 * 1024, MemFlags::default()),
            bus: Bus::new(),
        })
    }

    fn domapping(&mut self) {
        self.bus
            .map_mem(0x00000000, 0x03EFFFFF, &self.rdram)
            .unwrap();
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut hw = Hw::new();
    hw.domapping();

    let mut cpu = mips64::Cpu::new(/*bus.clone()*/);

    hw.bus.write::<u32>(0x01000234, 4);
    let val1 = hw.bus.read::<u16>(0x01000234);
    let val2 = hw.bus.read::<u16>(0x01000234 + 2);
    let val3 = hw.bus.fetch_read::<u16>(0x01000234 + 2).read();
    println!("Hello, world! {:x} / {:x} : {:x}", val1, val2, val3);

    cpu.step(args[1].parse::<u32>().expect("invalid argument"));
}
