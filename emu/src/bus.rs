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

pub trait Bus<'a> {
    type Order: ByteOrder;

    fn fetch_read(&'a self, pc: u32, size: AccessSize) -> MemIoR<'a>;
    fn fetch_write(&'a mut self, pc: u32, size: AccessSize) -> MemIoW<'a>;

    fn read8(&'a self, pc: u32) -> u8 {
        match self.fetch_read(pc, AccessSize::Size8) {
            MemIoR::Mem(buf) => buf[0],
            MemIoR::Func(f) => f() as u8,
            MemIoR::Unmapped() => 0xff,
        }
    }
    fn read16(&'a self, pc: u32) -> u16 {
        match self.fetch_read(pc, AccessSize::Size16) {
            MemIoR::Mem(buf) => Self::Order::read_u16(buf),
            MemIoR::Func(f) => f() as u16,
            MemIoR::Unmapped() => 0xffff,
        }
    }
    fn read32(&'a self, pc: u32) -> u32 {
        match self.fetch_read(pc, AccessSize::Size32) {
            MemIoR::Mem(buf) => Self::Order::read_u32(buf),
            MemIoR::Func(f) => f() as u32,
            MemIoR::Unmapped() => 0xffffffff,
        }
    }
    fn read64(&'a self, pc: u32) -> u64 {
        match self.fetch_read(pc, AccessSize::Size64) {
            MemIoR::Mem(buf) => Self::Order::read_u64(buf),
            MemIoR::Func(f) => f() as u64,
            MemIoR::Unmapped() => 0xffffffffffffffff,
        }
    }

    fn write8(&'a mut self, pc: u32, val: u8) {
        match self.fetch_write(pc, AccessSize::Size8) {
            MemIoW::Mem(ref mut buf) => (*buf)[0] = val,
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    fn write16(&'a mut self, pc: u32, val: u16) {
        match self.fetch_write(pc, AccessSize::Size16) {
            MemIoW::Mem(ref mut buf) => Self::Order::write_u16(*buf, val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    fn write32(&'a mut self, pc: u32, val: u32) {
        match self.fetch_write(pc, AccessSize::Size32) {
            MemIoW::Mem(ref mut buf) => Self::Order::write_u32(*buf, val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    fn write64(&'a mut self, pc: u32, val: u64) {
        match self.fetch_write(pc, AccessSize::Size64) {
            MemIoW::Mem(ref mut buf) => Self::Order::write_u64(*buf, val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
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

macro_rules! init_array(
    ($ty:ty, $len:expr, $val:expr) => (
        {
            let mut array: [$ty; $len] = unsafe { std::mem::uninitialized() };
            for i in array.iter_mut() {
                unsafe { ::std::ptr::write(i, $val); }
            }
            array
        }
    )
);

enum HwIo<'a> {
    Unmapped(),
    Mem(MemArea<'a>),
}

struct Node<'a> {
    ior: [HwIo<'a>; 256],
    iow: [HwIo<'a>; 256],
}

impl<'a> Node<'a> {
    fn new() -> Node<'a> {
        return Node {
            ior: init_array!(HwIo, 256, HwIo::Unmapped()),
            iow: init_array!(HwIo, 256, HwIo::Unmapped()),
        };
    }
}

pub struct Table<'a, O: ByteOrder> {
    arena: Vec<Box<Node<'a>>>,
    roots: EnumMap<AccessSize, Node<'a>>,

    phantom: PhantomData<O>,
}

impl<'a, O: ByteOrder> Table<'a, O> {
    pub fn new() -> Table<'a, O> {
        Table {
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
}

impl<'a, O: ByteOrder> Bus<'a> for Table<'a, O> {
    type Order = O;

    fn fetch_read(&'a self, addr: u32, size: AccessSize) -> MemIoR<'a> {
        match &self.roots[size].ior[(addr >> 24) as usize] {
            HwIo::Mem(mem) => mem.mem_io_r(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus read: addr={:x}", addr);
                MemIoR::Unmapped()
            }
        }
    }

    fn fetch_write(&'a mut self, addr: u32, size: AccessSize) -> MemIoW<'a> {
        match &mut self.roots[size].iow[(addr >> 24) as usize] {
            HwIo::Mem(mem) => mem.mem_io_w(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus write: addr={:x}", addr);
                MemIoW::Unmapped()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate byteorder;
    use self::byteorder::{ByteOrder, LittleEndian};

    #[test]
    fn table_mem() {
        let t: &Bus<Order = LittleEndian> = &Table::new();

        let val = t.read64(0x12000000);
        println!("{:x}", val);
        assert_eq!(4, 5);
    }
}
