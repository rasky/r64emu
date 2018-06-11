extern crate byteorder;

use super::memint::{MemInt,AccessSize,ByteOrderCombiner};
use self::byteorder::ByteOrder;
use enum_map::EnumMap;
use std;
use std::mem;
use std::slice;
use std::cell::RefCell;
use std::rc::Rc;
use std::marker::PhantomData;


pub struct RawPtr(pub *const u8);
pub struct RawPtrMut(pub *mut u8);

impl RawPtr {
    #[inline(always)]
    fn slice64(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(self.0, 8)
        }
    }
}

impl RawPtrMut {
    #[inline(always)]
    fn slice64(&mut self) -> &mut [u8] {
        unsafe {
            slice::from_raw_parts_mut(self.0, 8)
        }
    }
}

pub enum MemIoR<'a, O, U>
where
    O: ByteOrder,
    U: MemInt,
{
    Unmapped(PhantomData<O>, PhantomData<U>),
    Raw(RawPtr),
    Func(Box<'a + Fn() -> u64>),
}

impl<'a,O,U> MemIoR<'a, O, U>
where
    O: ByteOrder,
    U: MemInt,
{
    pub fn read(&self) -> U {
        match self {
            MemIoR::Raw(buf) => U::endian_read_from::<O>(buf.slice64()),
            MemIoR::Func(f) => U::truncate_from(f()),
            MemIoR::Unmapped(_,_) => U::truncate_from(0xffffffffffffffff),
        }
    }

    pub fn mem(&'a self) -> Option<&'a [u8]> {
        match self {
            MemIoR::Raw(buf) => Some(buf.slice64()),
            _ => None,
        }
    }
}

pub enum MemIoW<'a> {
    Unmapped(),
    Raw(RawPtrMut),
    Func(Box<'a + FnMut(u64)>),
}

impl<'a> MemIoW<'a> {
    fn mem(&'a mut self) -> Option<&'a mut [u8]> {
        match self {
            MemIoW::Raw(buf) => Some(buf.slice64()),
            _ => None,
        }
    }
}

struct MemArea {
    data: Rc<RefCell<[u8]>>,
    mask: u32,
}

impl MemArea {
    #[inline(always)]
    fn mem_io_r<'a, 'b:'a, O:ByteOrder, U:MemInt>(&'a self, mut pc: u32) -> MemIoR<'b,O,U> {
        pc &= self.mask;

        let buf = self.data.borrow();
        MemIoR::Raw::<O,U>(RawPtr(&buf[pc as usize]))
    }
    #[inline(always)]
    fn mem_io_w<'a, 'b:'a>(&'a mut self, mut pc: u32) -> MemIoW<'b> {
        pc &= self.mask;

        let mut buf = self.data.borrow_mut();
        MemIoW::Raw(RawPtrMut(&mut buf[pc as usize]))
    }
}

enum HwIo<'a> {
    Unmapped(),
    Combined(),
    Mem(Rc<RefCell<MemArea>>),
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

pub struct Bus<'a, Order: ByteOrder+ByteOrderCombiner> {
    nodes: Vec<Box<Node<'a>>>,
    mems: Vec<Rc<RefCell<MemArea>>>,

    roots: EnumMap<AccessSize, Box<Node<'a>>>,
    phantom: PhantomData<Order>,
}

impl<'a, Order> Bus<'a, Order>
where
    Order: ByteOrder+ByteOrderCombiner,
{
    pub fn new() -> Box<Bus<'a, Order>> {
        assert_eq_size!(HwIo, [u8; 16]);

        let b = box Bus {
            nodes: Vec::new(),
            mems: Vec::new(),
            roots: enum_map!{
                AccessSize::Size8 => Node::new(),
                AccessSize::Size16 => Node::new(),
                AccessSize::Size32 => Node::new(),
                AccessSize::Size64 => Node::new(),
            },
            phantom: PhantomData,
        };
        b
    }

    pub fn read<U:MemInt+'a>(&self, addr: u32) -> U {
        self.internal_fetch_read::<U>(addr).read()
    }

    pub fn write8(&mut self, pc: u32, val: u8) {
        match self.fetch_write(pc, AccessSize::Size8) {
            MemIoW::Raw(mut buf) => buf.slice64()[0] = val,
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn write16(&mut self, pc: u32, val: u16) {
        match self.fetch_write(pc, AccessSize::Size16) {
            MemIoW::Raw(mut buf) => Order::write_u16(buf.slice64(), val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn write32(&mut self, pc: u32, val: u32) {
        match self.fetch_write(pc, AccessSize::Size32) {
            MemIoW::Raw(mut buf) => Order::write_u32(buf.slice64(), val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn write64(&mut self, pc: u32, val: u64) {
        match self.fetch_write(pc, AccessSize::Size64) {
            MemIoW::Raw(mut buf) => Order::write_u64(buf.slice64(), val),
            MemIoW::Func(mut f) => f(val as u64),
            MemIoW::Unmapped() => {}
        }
    }

    pub fn fetch_read<U:MemInt+'a>(&self, addr: u32) -> MemIoR<Order,U> {
        self.internal_fetch_read::<U>(addr)
    }

    #[inline(always)]
    fn internal_fetch_read<U:MemInt+'a>(&self, addr: u32) -> MemIoR<Order,U> {
        let node = &self.roots[U::ACCESS_SIZE];
        let mut io = &node.ior[(addr >> 16) as usize];
        if let HwIo::Node(node) = io {
            io = &node.ior[(addr & 0xffff) as usize];
        }

        match io {
            HwIo::Mem(mem) => mem.borrow().mem_io_r(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus read: addr={:x}", addr);
                MemIoR::Unmapped(PhantomData, PhantomData)
            }
            HwIo::Combined() => self.fetch_read_combined::<U>(addr),
            HwIo::Node(_) => panic!("internal error: invalid bus table"),
        }
    }

    #[inline(always)]
    fn fetch_write(&mut self, addr: u32, size: AccessSize) -> MemIoW {
        let node = &mut self.roots[size];
        let io = &mut node.iow[(addr >> 16) as usize];

        match io {
            HwIo::Mem(mem) => mem.borrow_mut().mem_io_w(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus write: addr={:x}", addr);
                MemIoW::Unmapped()
            }
            HwIo::Combined() => unimplemented!(),
            HwIo::Node(node) => match &mut node.iow[(addr & 0xffff) as usize] {
                HwIo::Mem(mem) => mem.borrow_mut().mem_io_w(addr),
                HwIo::Unmapped() => {
                    println!("unmapped bus write: addr={:x}", addr);
                    MemIoW::Unmapped()
                }
                HwIo::Node(_) => panic!("internal error: invalid bus table"),
                HwIo::Combined() => unimplemented!(),
            },
        }
    }

    pub fn map_reg32(&mut self) {
        self.roots[AccessSize::Size32].ior[0x1234] = HwIo::Combined()
    }

    pub fn map_mem(&mut self, begin: u32, end: u32, buf: Rc<RefCell<[u8]>>) -> Result<(), &str> {
        let pmemsize = buf.borrow().len();
        if pmemsize & (pmemsize-1) != 0 {
            return Err("map_mem: memory buffer should be a power of two");
        }

        let mem = Rc::new(RefCell::new(MemArea{
            data: buf,
            mask: (pmemsize-1) as u32,
        }));

        let vmemsize = end-begin+1;
        if vmemsize < 0x10000 {
            unimplemented!();

        } else {
            if (begin&0xffff) != 0 || (end&0xffff) != 0xffff {
                unimplemented!();
            }

            for idx in begin>>16..(end>>16)+1 {
                for sz in vec![AccessSize::Size8, AccessSize::Size16, AccessSize::Size32, AccessSize::Size64] {
                    self.roots[sz].ior[idx as usize] = HwIo::Mem(mem.clone());
                    self.roots[sz].iow[idx as usize] = HwIo::Mem(mem.clone());
                }
            }
        }

        self.mems.push(mem);
        return Ok(());
    }

    fn fetch_read_combined<U:MemInt+'a>(&self, addr: u32) -> MemIoR<Order,U> {
        let before = self.fetch_read::<U::Half>(addr);
        let after = self.fetch_read::<U::Half>(addr+(mem::size_of::<U>() as u32)/2);

        MemIoR::Func(Box::new(move || {
            U::from_halves::<Order>(before.read(), after.read()).into()
        }))
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
