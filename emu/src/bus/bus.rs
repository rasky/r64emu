extern crate byteorder;

use self::byteorder::ByteOrder;
use super::memint::{AccessSize, ByteOrderCombiner, MemInt};
use super::radix::RadixTree;
use super::regs::Reg;
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
}

impl<'a, 'b> Clone for HwIoR<'a>
where
    'a: 'b,
{
    fn clone(&self) -> HwIoR<'a> {
        match self {
            HwIoR::Mem(ref mem, mask) => HwIoR::Mem(mem, *mask),
            HwIoR::Func(f) => HwIoR::Func(f.clone()),
        }
    }
}

pub enum HwIoW<'a> {
    Mem(&'a RefCell<[u8]>, u32),
    Func(Rc<'a + Fn(u32, u64)>),
}

impl<'a> Clone for HwIoW<'a> {
    fn clone(&self) -> HwIoW<'a> {
        match self {
            HwIoW::Mem(ref mem, mask) => HwIoW::Mem(mem, *mask),
            HwIoW::Func(f) => HwIoW::Func(f.clone()),
        }
    }
}

impl<'a> HwIoR<'a> {
    pub fn at<O: ByteOrder, U: MemInt>(&'a self, addr: u32) -> MemIoR<'a, O, U> {
        MemIoR {
            hwio: self,
            addr,
            phantom: PhantomData,
        }
    }
}

impl<'a> HwIoW<'a> {
    pub fn at<O: ByteOrder, U: MemInt>(&'a self, addr: u32) -> MemIoW<'a, O, U> {
        MemIoW {
            hwio: self,
            addr,
            phantom: PhantomData,
        }
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
            HwIoW::Mem(buf, mask) => {
                U::endian_write_to::<O>(&mut buf.borrow_mut()[(addr & mask) as usize..], val)
            }
            HwIoW::Func(f) => f(addr, val.into()),
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

pub struct Bus<'a, Order: ByteOrderCombiner + 'a> {
    reads: EnumMap<AccessSize, Box<RadixTree<HwIoR<'a>>>>,
    writes: EnumMap<AccessSize, Box<RadixTree<HwIoW<'a>>>>,

    unmap_r: HwIoR<'a>,
    unmap_w: HwIoW<'a>,

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
                AccessSize::Size8 => RadixTree::new(),
                AccessSize::Size16 => RadixTree::new(),
                AccessSize::Size32 => RadixTree::new(),
                AccessSize::Size64 => RadixTree::new(),
            },
            writes: enum_map!{
                AccessSize::Size8 => RadixTree::new(),
                AccessSize::Size16 => RadixTree::new(),
                AccessSize::Size32 => RadixTree::new(),
                AccessSize::Size64 => RadixTree::new(),
            },
            unmap_r: unmapped_area_r(),
            unmap_w: unmapped_area_w(),
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
        self.reads[U::ACCESS_SIZE]
            .lookup(addr)
            .or(Some(&self.unmap_r))
            .unwrap()
            .at(addr)
    }

    #[inline(always)]
    fn internal_fetch_write<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoW<'b, Order, U> {
        self.writes[U::ACCESS_SIZE]
            .lookup(addr)
            .or(Some(&self.unmap_w))
            .unwrap()
            .at(addr)
    }

    fn mapreg_partial<'s, 'r: 's, U, S>(
        &'s mut self,
        addr: u32,
        reg: &'a Reg<Order, U>,
    ) -> Result<(), &'r str>
    where
        U: MemInt,
        S: MemInt + Into<U>,
    {
        self.reads[S::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            reg.hw_io_r::<S>(),
        )?;
        self.writes[S::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            reg.hw_io_w::<S>(),
        )?;
        Ok(())
    }

    pub fn map_reg8(&mut self, addr: u32, reg: &'a Reg<Order, u8>) -> Result<(), &str> {
        self.mapreg_partial::<u8, u8>(addr, reg)?;
        Ok(())
    }

    pub fn map_reg16<'s, 'r: 's>(
        &'s mut self,
        addr: u32,
        reg: &'a Reg<Order, u16>,
    ) -> Result<(), &'r str> {
        self.mapreg_partial::<u16, u8>(addr, reg)?;
        self.mapreg_partial::<u16, u16>(addr, reg)?;
        Ok(())
    }

    pub fn map_reg32<'s, 'r: 's>(
        &'s mut self,
        addr: u32,
        reg: &'a Reg<Order, u32>,
    ) -> Result<(), &'r str> {
        self.mapreg_partial::<u32, u8>(addr, reg)?;
        self.mapreg_partial::<u32, u16>(addr, reg)?;
        self.mapreg_partial::<u32, u32>(addr, reg)?;
        Ok(())
    }

    pub fn map_reg64<'s, 'r: 's>(
        &'s mut self,
        addr: u32,
        reg: &'a Reg<Order, u64>,
    ) -> Result<(), &'r str> {
        self.mapreg_partial::<u64, u8>(addr, reg)?;
        self.mapreg_partial::<u64, u16>(addr, reg)?;
        self.mapreg_partial::<u64, u32>(addr, reg)?;
        self.mapreg_partial::<u64, u64>(addr, reg)?;
        Ok(())
    }

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

        for (_, node) in self.reads.iter_mut() {
            node.insert_range(begin, end, HwIoR::Mem(buf, mask))?;
        }
        if !readonly {
            for (_, node) in self.writes.iter_mut() {
                node.insert_range(begin, end, HwIoW::Mem(buf, mask))?;
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
    use bus::le::{Reg32, RegFlags};
    use std::mem;

    extern crate byteorder;
    use self::byteorder::LittleEndian;

    #[test]
    fn basic_mem() {
        let ram1 = RefCell::new([0u8; 1024]);
        let mut bus = Bus::<LittleEndian>::new();

        assert_eq!(
            bus.map_mem(0x04000000, 0x06000000, &ram1, false).is_ok(),
            true
        );
        bus.write::<u32>(0x04000123, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x04000123), 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x05aaa123), 0xaabbccdd);
        assert_eq!(bus.read::<u8>(0x04bbb125), 0xbb);
    }

    #[test]
    fn basic_reg() {
        let reg1 = Reg32::default();
        reg1.set(0x12345678);

        let reg2 = Reg32::new(
            0x12345678,
            0xffff0000,
            RegFlags::default(),
            None,
            Some(box |x| x | 0xf0),
        );

        let mut bus = Bus::<LittleEndian>::new();
        assert_eq!(bus.map_reg32(0x04000120, &reg1).is_ok(), true);
        assert_eq!(bus.map_reg32(0x04000124, &reg2).is_ok(), true);

        assert_eq!(bus.read::<u32>(0x04000120), 0x12345678);
        assert_eq!(bus.read::<u16>(0x04000122), 0x1234);
        assert_eq!(bus.read::<u8>(0x04000121), 0x56);

        assert_eq!(bus.read::<u32>(0x04000124), 0x123456f8);
        bus.write::<u8>(0x04000124, 0x00);
        bus.write::<u16>(0x04000126, 0xaabb);
        assert_eq!(bus.read::<u32>(0x04000124), 0xaabb56f8);

        // assert_eq!(bus.read::<u64>(0x04000120), 0xaabb56f812345678);
    }
}
