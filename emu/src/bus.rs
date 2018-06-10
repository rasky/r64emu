extern crate byteorder;

use self::byteorder::ByteOrder;
use enum_map::EnumMap;
use std;
use std::marker::PhantomData;

#[derive(Debug, Enum)]
pub enum AccessSize {
    Size8,
    Size16,
    Size32,
    Size64,
}

pub enum MemIoR<'a> {
    Unmapped(),
    Mem(&'a [u8]),
    Func(Box<'a + Fn() -> u64>),
}

pub enum MemIoW<'a> {
    Unmapped(),
    Mem(&'a mut [u8]),
    Func(Box<'a + FnMut(u64)>),
}

impl<'a> MemIoR<'a> {
    fn mem(&'a self) -> Option<&'a [u8]> {
        match self {
            MemIoR::Mem(buf) => Some(buf),
            _ => None,
        }
    }
}

impl<'a> MemIoW<'a> {
    fn mem(&'a mut self) -> Option<&'a mut [u8]> {
        match self {
            MemIoW::Mem(buf) => Some(buf),
            _ => None,
        }
    }
}

struct MemArea<'a> {
    data: &'a mut [u8],
    mask: u32,
}

impl<'a> MemArea<'a> {
    #[inline(always)]
    fn mem_io_r(&'a self, mut pc: u32) -> MemIoR<'a> {
        pc &= self.mask;
        MemIoR::Mem(&self.data[pc as usize..])
    }
    #[inline(always)]
    fn mem_io_w(&'a mut self, mut pc: u32) -> MemIoW<'a> {
        pc &= self.mask;
        MemIoW::Mem(&mut self.data[pc as usize..])
    }
}

#[repr(u8)]
enum HwIo<'a> {
    Unmapped(),
    Mem(&'a mut MemArea<'a>),
    Node(&'a mut Node<'a>),
}

struct Node<'a> {
    ior: [HwIo<'a>; 65536],
    iow: [HwIo<'a>; 65536],
}

impl<'a> Node<'a> {
    fn new() -> Box<Node<'a>> {
        let mut n = box Node {
            ior: unsafe { std::mem::uninitialized() },
            iow: unsafe { std::mem::uninitialized() },
        };

        for i in 0..n.ior.len() {
            n.ior[i] = HwIo::Unmapped();
            n.iow[i] = HwIo::Unmapped();
        }

        n
    }
}

pub struct Bus<'a, Order: ByteOrder> {
    arena: Vec<Box<Node<'a>>>,
    roots: EnumMap<AccessSize, Box<Node<'a>>>,
    phantom: PhantomData<Order>,
}

impl<'a, Order> Bus<'a, Order>
where
    Order: ByteOrder,
{
    pub fn read8(&'a self, pc: u32) -> u8 {
        match self.fetch_read(pc, AccessSize::Size8) {
            MemIoR::Mem(buf) => buf[0],
            MemIoR::Func(f) => f() as u8,
            MemIoR::Unmapped() => 0xff,
        }
    }
    pub fn read16(&'a self, pc: u32) -> u16 {
        match self.fetch_read(pc, AccessSize::Size16) {
            MemIoR::Mem(buf) => Order::read_u16(buf),
            MemIoR::Func(f) => f() as u16,
            MemIoR::Unmapped() => 0xffff,
        }
    }
    pub fn read32(&self, pc: u32) -> u32 {
        match self.fetch_read(pc, AccessSize::Size32) {
            MemIoR::Mem(buf) => Order::read_u32(buf),
            MemIoR::Func(f) => f() as u32,
            MemIoR::Unmapped() => 0xffffffff,
        }
    }
    pub fn read64(&'a self, pc: u32) -> u64 {
        match self.fetch_read(pc, AccessSize::Size64) {
            MemIoR::Mem(buf) => Order::read_u64(buf),
            MemIoR::Func(f) => f() as u64,
            MemIoR::Unmapped() => 0xffffffffffffffff,
        }
    }

    pub fn write8(&'a mut self, pc: u32, val: u8) {
        match self.fetch_write(pc, AccessSize::Size8) {
            MemIoW::Mem(ref mut buf) => (*buf)[0] = val,
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn write16(&'a mut self, pc: u32, val: u16) {
        match self.fetch_write(pc, AccessSize::Size16) {
            MemIoW::Mem(ref mut buf) => Order::write_u16(*buf, val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn write32(&'a mut self, pc: u32, val: u32) {
        match self.fetch_write(pc, AccessSize::Size32) {
            MemIoW::Mem(ref mut buf) => Order::write_u32(*buf, val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn write64(&'a mut self, pc: u32, val: u64) {
        match self.fetch_write(pc, AccessSize::Size64) {
            MemIoW::Mem(ref mut buf) => Order::write_u64(*buf, val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn new() -> Box<Bus<'a, Order>> {
        box Bus {
            arena: Vec::new(),
            roots: enum_map!{
                AccessSize::Size8 => Node::new(),
                AccessSize::Size16 => Node::new(),
                AccessSize::Size32 => Node::new(),
                AccessSize::Size64 => Node::new(),
            },
            phantom: PhantomData,
        }
    }

    fn fetch_read<'b>(&'b self, addr: u32, size: AccessSize) -> MemIoR<'b> {
        let node = &self.roots[size];
        let mut io = &node.ior[(addr >> 16) as usize];
        if let HwIo::Node(node) = io {
            io = &node.ior[(addr & 0xffff) as usize];
        }

        match io {
            HwIo::Mem(mem) => mem.mem_io_r(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus read: addr={:x}", addr);
                MemIoR::Unmapped()
            }
            HwIo::Node(_) => panic!("internal error: invalid bus table"),
        }
    }

    fn fetch_write(&'a mut self, addr: u32, size: AccessSize) -> MemIoW<'a> {
        let node = &mut self.roots[size];
        let io = &mut node.iow[(addr >> 16) as usize];

        match io {
            HwIo::Mem(mem) => mem.mem_io_w(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus write: addr={:x}", addr);
                MemIoW::Unmapped()
            }
            HwIo::Node(node) => match &mut node.iow[(addr & 0xffff) as usize] {
                HwIo::Mem(mem) => mem.mem_io_w(addr),
                HwIo::Unmapped() => {
                    println!("unmapped bus write: addr={:x}", addr);
                    MemIoW::Unmapped()
                }
                HwIo::Node(_) => panic!("internal error: invalid bus table"),
            },
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    extern crate byteorder;
    use self::byteorder::{ByteOrder, LittleEndian};

    #[test]
    fn table_mem() {
        let t = &Bus::<LittleEndian>::new();

        println!("sizeof HwIo: {}", mem::size_of::<HwIo>());

        let val = t.read64(0x12000000);
        println!("{:x}", val);
        assert_eq!(4, 5);
    }
}
