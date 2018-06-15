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
    pub fn read<'b>(&'b self) -> U {
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
    Node(Box<Node<'a>>),
}

struct Node<'a> {
    io: [HwIo<'a>; 65536],
}

impl<'a> Node<'a> {
    fn new() -> Box<Node<'a>> {
        let mut n = box Node {
            io: unsafe { std::mem::uninitialized() },
        };

        for i in 0..n.io.len() {
            n.io[i] = HwIo::Unmapped();
        }

        return n;
    }
}

pub struct Bus<'a, Order: ByteOrderCombiner+'a> {
    roots: EnumMap<AccessSize, Box<Node<'a>>>,

    phantom: PhantomData<Order>,
}

impl<'a, 'b, 'c:'b, Order: 'c> Bus<'a, Order>
where
    Order: ByteOrderCombiner+'a
{
    pub fn new() -> Box<Bus<'a, Order>> {
        assert_eq_size!(HwIo, [u8; 16]);

        Box::new(Bus {
            roots: enum_map!{
                AccessSize::Size8 => Node::new(),
                AccessSize::Size16 => Node::new(),
                AccessSize::Size32 => Node::new(),
                AccessSize::Size64 => Node::new(),
            },
            phantom: PhantomData,
        })
    }

    pub fn read<U: MemInt + 'a>(&self, addr: u32) -> U {
        self.internal_fetch_read::<U>(addr).read()
    }

    pub fn write<U: MemInt + 'a>(&mut self, addr: u32, val: U) {
        self.internal_fetch_write::<U>(addr).write(val);
    }

    #[inline(never)]
    pub fn fetch_read<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoR<'c, Order, U> {
        self.internal_fetch_read::<U>(addr)
    }

    #[inline(never)]
    pub fn fetch_write<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoW<'c, Order, U> {
        self.internal_fetch_write::<U>(addr)
    }

    #[inline(always)]
    fn internal_fetch_read<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoR<'c, Order, U> {
        let node = &self.roots[U::ACCESS_SIZE];
        let mut io = &node.io[(addr >> 16) as usize];
        if let HwIo::Node(node) = io {
            io = &node.io[(addr & 0xffff) as usize];
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
    fn internal_fetch_write<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoW<'c, Order, U> {
        let node = &self.roots[U::ACCESS_SIZE];
        let mut io = &node.io[(addr >> 16) as usize];
        if let HwIo::Node(node) = io {
            io = &node.io[(addr & 0xffff) as usize];
        }

        match io {
            HwIo::Mem(mem) => mem.mem_io_w(addr),
            HwIo::Unmapped() => {
                println!("unmapped bus write: addr={:x}", addr);
                MemIoW::Unmapped(PhantomData, PhantomData)
            }
            HwIo::Combined() => self.fetch_write_combined::<U>(addr),
            HwIo::Node(_) => panic!("internal error: invalid bus table"),
        }
    }

    pub fn map_reg32(&mut self) {
        self.roots[AccessSize::Size32].io[0x1234] = HwIo::Combined()
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
                    self.roots[*sz].io[idx as usize] = HwIo::Mem(mem.clone());
                }
            }
        }

        return Ok(());
    }

    fn fetch_read_combined<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoR<'c, Order, U> {
        let before = self.fetch_read::<U::Half>(addr);
        let after = self.fetch_read::<U::Half>(addr + (mem::size_of::<U>() as u32) / 2);

        MemIoR::Func(Box::new(move || {
            U::from_halves::<Order>(before.read(), after.read()).into()
        }))
    }

    fn fetch_write_combined<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoW<'c, Order, U> {
        let off = (mem::size_of::<U>() as u32) / 2;

        let mut before = self.fetch_write::<U::Half>(addr);
        let mut after = self.fetch_write::<U::Half>(addr+off);

        MemIoW::Func(Box::new(move |val64| {
            let (mask1, shift1) = Order::subint_mask::<U,U::Half>(0);
            let (mask2, shift2) = Order::subint_mask::<U,U::Half>(off as usize);
            let val_before = (val64 & mask1.into()) >> shift1;
            let val_after = (val64 & mask2.into()) >> shift2;
            before.write(U::Half::truncate_from(val_before));
            after.write(U::Half::truncate_from(val_after));
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    extern crate byteorder;
    use self::byteorder::LittleEndian;

    #[test]
    fn table_mem() {
        let t = &Bus::<LittleEndian>::new();

        println!("sizeof HwIo: {}", mem::size_of::<HwIo>());
    }
}
