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

pub enum HwIoR<'a> {
    Mem(&'a RefCell<[u8]>, u32),
    Func(Rc<'a + Fn(u32) -> u64>),
    Node(Box<NodeR<'a>>),
}

pub enum HwIoW<'a> {
    Mem(&'a RefCell<[u8]>, u32),
    Func(Rc<'a + Fn(u32, u64)>),
    Node(Box<NodeW<'a>>),
}

impl<'a> HwIoR<'a> {
    pub fn at<O:ByteOrder, U:MemInt>(&'a self, addr: u32) -> MemIoR<'a, O, U> {
        MemIoR{hwio: self, addr, phantom: PhantomData}
    }
}

impl<'a> HwIoW<'a> {
    pub fn at<O:ByteOrder, U:MemInt>(&'a self, addr: u32) -> MemIoW<'a, O, U> {
        MemIoW{hwio: self, addr, phantom: PhantomData}
    }
}

pub struct MemIoR<'a, O: ByteOrder, U: MemInt> {
    hwio: &'a HwIoR<'a>,
    addr: u32,
    phantom: PhantomData<(O, U)>,
}

pub struct MemIoW<'a, O: ByteOrder, U: MemInt> {
    hwio: &'a HwIoW<'a>,
    addr: u32,
    phantom: PhantomData<(O, U)>,
}

impl<'a, O, U> MemIoR<'a, O, U>
where
    O: ByteOrder,
    U: MemInt,
{
    #[inline(always)]
    pub fn read<'b>(&'b self) -> U {
        let addr = self.addr;
        match self.hwio {
            HwIoR::Mem(buf, mask) => {
                U::endian_read_from::<O>(&buf.borrow()[(addr & mask) as usize..])
            }
            HwIoR::Func(f) => U::truncate_from(f(addr)),
            _ => unreachable!(),
        }
    }
}

impl<'a, O, U> MemIoW<'a, O, U>
where
    O: ByteOrder,
    U: MemInt,
{
    #[inline(always)]
    pub fn write(&self, val: U) {
        let addr = self.addr;
        match self.hwio {
            HwIoW::Mem(buf, mask) => U::endian_write_to::<O>(
                &mut buf.borrow_mut()[(addr & mask) as usize..],
                val,
            ),
            HwIoW::Func(f) => f(addr, val.into()),
            _ => unreachable!(),
        }
    }
}

pub fn unmapped_area_r<'a>() -> HwIoR<'a> {
    thread_local!(
        static FN: Rc<Fn(u32)->u64> = Rc::new(|_| {
            // FIXME: log
            return 0xffffffffffffffff;
        })
    );
    HwIoR::Func(FN.with(|c| c.clone()))
}

pub fn unmapped_area_w<'a>() -> HwIoW<'a> {
    thread_local!(
        static FN: Rc<Fn(u32,u64)> = Rc::new(|_,_| {
            // FIXME: log
        })
    );
    HwIoW::Func(FN.with(|c| c.clone()))
}

pub struct NodeR<'a> {
    io: [HwIoR<'a>; 65536],
}

pub struct NodeW<'a> {
    io: [HwIoW<'a>; 65536],
}

impl<'a> NodeW<'a> {
    fn new() -> Box<NodeW<'a>> {
        let mut n = box NodeW {
            io: unsafe { std::mem::uninitialized() },
        };

        for i in 0..n.io.len() {
            n.io[i] = unmapped_area_w();
        }

        return n;
    }
}

impl<'a> NodeR<'a> {
    fn new() -> Box<NodeR<'a>> {
        let mut n = box NodeR {
            io: unsafe { std::mem::uninitialized() },
        };

        for i in 0..n.io.len() {
            n.io[i] = unmapped_area_r();
        }

        return n;
    }
}

pub struct Bus<'a, Order: ByteOrderCombiner + 'a> {
    reads: EnumMap<AccessSize, Box<NodeR<'a>>>,
    writes: EnumMap<AccessSize, Box<NodeW<'a>>>,

    phantom: PhantomData<Order>,
}

impl<'a, 'b, 'c: 'b, Order: 'c> Bus<'a, Order>
where
    Order: ByteOrderCombiner + 'a,
{
    pub fn new() -> Box<Bus<'a, Order>> {

        assert_eq_size!(HwIoR, [u8; 24]);
        assert_eq_size!(HwIoW, [u8; 24]);

        Box::new(Bus {
            reads: enum_map!{
                AccessSize::Size8 => NodeR::new(),
                AccessSize::Size16 => NodeR::new(),
                AccessSize::Size32 => NodeR::new(),
                AccessSize::Size64 => NodeR::new(),
            },
            writes: enum_map!{
                AccessSize::Size8 => NodeW::new(),
                AccessSize::Size16 => NodeW::new(),
                AccessSize::Size32 => NodeW::new(),
                AccessSize::Size64 => NodeW::new(),
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
    pub fn fetch_read<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoR<'b, Order, U> {
        self.internal_fetch_read::<U>(addr)
    }

    #[inline(never)]
    pub fn fetch_write<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoW<'b, Order, U> {
        self.internal_fetch_write::<U>(addr)
    }

    #[inline(always)]
    fn internal_fetch_read<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoR<'b, Order, U> {
        let node = &self.reads[U::ACCESS_SIZE];
        let mut io = &node.io[(addr >> 16) as usize];
        while let HwIoR::Node(node) = io {
            io = &node.io[(addr & 0xffff) as usize];
        }
        io.at(addr)
    }

    #[inline(always)]
    fn internal_fetch_write<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoW<'b, Order, U> {
        let node = &self.writes[U::ACCESS_SIZE];
        let mut io = &node.io[(addr >> 16) as usize];
        while let HwIoW::Node(node) = io {
            io = &node.io[(addr & 0xffff) as usize];
        }
        io.at(addr)
    }

    // pub fn map_reg32(&mut self) {
    //     self.roots[AccessSize::Size32].io[0x1234] = HwIo::Combined()
    // }

    pub fn map_mem(
        &mut self,
        begin: u32,
        end: u32,
        buf: &'a RefCell<[u8]>,
        readonly: bool,
    ) -> Result<(), &str> {
        let pmemsize = buf.borrow().len();
        if pmemsize & (pmemsize - 1) != 0 {
            return Err("map_mem: memory buffer should be a power of two");
        }

        let mask = (pmemsize - 1) as u32;

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
                ].iter()
                {
                    self.reads[*sz].io[idx as usize] = HwIoR::Mem(buf, mask);
                    if !readonly {
                        self.writes[*sz].io[idx as usize] = HwIoW::Mem(buf, mask);
                    }
                }
            }
        }

        return Ok(());
    }

    // fn fetch_read_combined<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoR<'c, Order, U> {
    //     let before = self.fetch_read::<U::Half>(addr);
    //     let after = self.fetch_read::<U::Half>(addr + (mem::size_of::<U>() as u32) / 2);

    //     MemIoR::Func(Box::new(move || {
    //         U::from_halves::<Order>(before.read(), after.read()).into()
    //     }))
    // }

    // fn fetch_write_combined<U: MemInt + 'c>(&'b self, addr: u32) -> MemIoW<'c, Order, U> {
    //     let off = (mem::size_of::<U>() as u32) / 2;

    //     let mut before = self.fetch_write::<U::Half>(addr);
    //     let mut after = self.fetch_write::<U::Half>(addr+off);

    //     MemIoW::Func(Box::new(move |val64| {
    //         let (mask1, shift1) = Order::subint_mask::<U,U::Half>(0);
    //         let (mask2, shift2) = Order::subint_mask::<U,U::Half>(off as usize);
    //         let val_before = (val64 & mask1.into()) >> shift1;
    //         let val_after = (val64 & mask2.into()) >> shift2;
    //         before.write(U::Half::truncate_from(val_before));
    //         after.write(U::Half::truncate_from(val_after));
    //     }))
    // }
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
