extern crate byteorder;

use self::byteorder::ByteOrder;
use super::memint::{AccessSize, ByteOrderCombiner, MemInt};
use enum_map::EnumMap;
use std;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;
use std::slice;

pub struct RawPtr(pub *const u8);
pub struct RawPtrMut(pub *mut u8);

impl RawPtr {
    #[inline(always)]
    fn slice_for<O: MemInt>(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.0, mem::size_of::<O>()) }
    }
}

impl RawPtrMut {
    #[inline(always)]
    fn slice_for<O: MemInt>(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.0, mem::size_of::<O>()) }
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

impl<'a, O, U> MemIoR<'a, O, U>
where
    O: ByteOrder,
    U: MemInt,
{
    #[inline(always)]
    pub fn read(&self) -> U {
        match self {
            MemIoR::Raw(buf) => U::endian_read_from::<O>(buf.slice_for::<U>()),
            MemIoR::Func(f) => U::truncate_from(f()),
            MemIoR::Unmapped(_, _) => U::truncate_from(0xffffffffffffffff),
        }
    }
}

pub enum MemIoW<'a, O: ByteOrder, U: MemInt> {
    Unmapped(PhantomData<O>, PhantomData<U>),
    Raw(RawPtrMut),
    Func(Box<'a + FnMut(u64)>),
}

impl<'a, O, U> MemIoW<'a, O, U>
where
    O: ByteOrder,
    U: MemInt,
{
    #[inline(always)]
    pub fn write(&mut self, val: U) {
        match self {
            MemIoW::Raw(ref mut buf) => U::endian_write_to::<O>(buf.slice_for::<U>(), val),
            MemIoW::Func(ref mut f) => f(val.into()),
            MemIoW::Unmapped(_, _) => {}
        }
    }
}

struct MemArea<'m> {
    data: &'m RefCell<[u8]>,
    mask: u32,
}

impl<'m> MemArea<'m> {
    #[inline(always)]
    fn mem_io_r<'a, 'b: 'a, O: ByteOrder, U: MemInt>(&'a self, mut pc: u32) -> MemIoR<'b, O, U> {
        pc &= self.mask;

        let buf = self.data.borrow();
        MemIoR::Raw::<O, U>(RawPtr(&buf[pc as usize]))
    }
    #[inline(always)]
    fn mem_io_w<'a, 'b: 'a, O: ByteOrder, U: MemInt>(&'a self, mut pc: u32) -> MemIoW<'b, O, U> {
        pc &= self.mask;

        let mut buf = self.data.borrow_mut();
        MemIoW::Raw::<O, U>(RawPtrMut(&mut buf[pc as usize]))
    }
}

enum HwIo<'a> {
    Unmapped(),
    Combined(),
    Mem(Rc<MemArea<'a>>),
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

pub struct Bus<'a, Order: ByteOrder + ByteOrderCombiner> {
    nodes: Vec<Box<Node<'a>>>,

    roots: EnumMap<AccessSize, Box<Node<'a>>>,
    phantom: PhantomData<Order>,
}

impl<'a, Order> Bus<'a, Order>
where
    Order: ByteOrder + ByteOrderCombiner,
{
    pub fn new() -> Box<Bus<'a, Order>> {
        assert_eq_size!(HwIo, [u8; 16]);

        let b = box Bus {
            nodes: Vec::new(),
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

    pub fn read<U: MemInt + 'a>(&self, addr: u32) -> U {
        self.internal_fetch_read::<U>(addr).read()
    }

    pub fn write<U: MemInt + 'a>(&mut self, addr: u32, val: U) {
        self.internal_fetch_write::<U>(addr).write(val);
    }

    #[inline(never)]
    pub fn fetch_read<U: MemInt + 'a>(&self, addr: u32) -> MemIoR<Order, U> {
        self.internal_fetch_read::<U>(addr)
    }

    #[inline(never)]
    pub fn fetch_write<U: MemInt + 'a>(&mut self, addr: u32) -> MemIoW<Order, U> {
        self.internal_fetch_write::<U>(addr)
    }

    #[inline(always)]
    fn internal_fetch_read<U: MemInt + 'a>(&self, addr: u32) -> MemIoR<Order, U> {
        let node = &self.roots[U::ACCESS_SIZE];
        let mut io = &node.ior[(addr >> 16) as usize];
        if let HwIo::Node(node) = io {
            io = &node.ior[(addr & 0xffff) as usize];
        }

        match io {
            HwIo::Mem(mem) => mem.mem_io_r(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus read: addr={:x}", addr);
                MemIoR::Unmapped(PhantomData, PhantomData)
            }
            HwIo::Combined() => self.fetch_read_combined::<U>(addr),
            HwIo::Node(_) => panic!("internal error: invalid bus table"),
        }
    }

    #[inline(always)]
    fn internal_fetch_write<U: MemInt + 'a>(&mut self, addr: u32) -> MemIoW<Order, U> {
        let node = &mut self.roots[U::ACCESS_SIZE];
        let io = &mut node.iow[(addr >> 16) as usize];

        match io {
            HwIo::Mem(mem) => mem.mem_io_w(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus write: addr={:x}", addr);
                MemIoW::Unmapped(PhantomData, PhantomData)
            }
            HwIo::Combined() => unimplemented!(),
            HwIo::Node(node) => match &mut node.iow[(addr & 0xffff) as usize] {
                HwIo::Mem(mem) => mem.mem_io_w(addr),
                HwIo::Unmapped() => {
                    println!("unmapped bus write: addr={:x}", addr);
                    MemIoW::Unmapped(PhantomData, PhantomData)
                }
                HwIo::Node(_) => panic!("internal error: invalid bus table"),
                HwIo::Combined() => unimplemented!(),
            },
        }
    }

    pub fn map_reg32(&mut self) {
        self.roots[AccessSize::Size32].ior[0x1234] = HwIo::Combined()
    }

    pub fn map_mem(&mut self, begin: u32, end: u32, buf: &'a RefCell<[u8]>) -> Result<(), &str> {
        let pmemsize = buf.borrow().len();
        if pmemsize & (pmemsize - 1) != 0 {
            return Err("map_mem: memory buffer should be a power of two");
        }

        let mem = Rc::new(MemArea {
            data: buf,
            mask: (pmemsize - 1) as u32,
        });

        let vmemsize = end - begin + 1;
        if vmemsize < 0x10000 {
            unimplemented!();
        } else {
            if (begin & 0xffff) != 0 || (end & 0xffff) != 0xffff {
                unimplemented!();
            }

            for idx in begin >> 16..(end >> 16) + 1 {
                for sz in [
                    AccessSize::Size8,
                    AccessSize::Size16,
                    AccessSize::Size32,
                    AccessSize::Size64,
                ].iter() {
                    self.roots[*sz].ior[idx as usize] = HwIo::Mem(mem.clone());
                    self.roots[*sz].iow[idx as usize] = HwIo::Mem(mem.clone());
                }
            }
        }

        return Ok(());
    }

    fn fetch_read_combined<U: MemInt + 'a>(&self, addr: u32) -> MemIoR<Order, U> {
        let before = self.fetch_read::<U::Half>(addr);
        let after = self.fetch_read::<U::Half>(addr + (mem::size_of::<U>() as u32) / 2);

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
