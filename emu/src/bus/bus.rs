extern crate byteorder;

use self::byteorder::ByteOrder;
use super::memint::{AccessSize, ByteOrderCombiner, MemInt};
use super::radix::RadixTree;
use super::regs::Reg;
use enum_map::EnumMap;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;

pub enum HwIoR<'a> {
    Mem(&'a RefCell<[u8]>, u32),
    Func(Rc<'a + Fn(u32) -> u64>),
}

impl<'a> Clone for HwIoR<'a> {
    #[inline(always)]
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
    #[inline(always)]
    fn clone(&self) -> HwIoW<'a> {
        match self {
            HwIoW::Mem(ref mem, mask) => HwIoW::Mem(mem, *mask),
            HwIoW::Func(f) => HwIoW::Func(f.clone()),
        }
    }
}

impl<'a> HwIoR<'a> {
    pub fn at<O: ByteOrder, U: MemInt>(self, addr: u32) -> MemIoR<'a, O, U> {
        MemIoR {
            hwio: self,
            addr,
            phantom: PhantomData,
        }
    }
}

impl<'a> HwIoW<'a> {
    pub fn at<O: ByteOrder, U: MemInt>(self, addr: u32) -> MemIoW<'a, O, U> {
        MemIoW {
            hwio: self,
            addr,
            phantom: PhantomData,
        }
    }
}

pub struct MemIoR<'a, O: ByteOrder, U: MemInt> {
    hwio: HwIoR<'a>,
    addr: u32,
    phantom: PhantomData<(O, U)>,
}

pub struct MemIoW<'a, O: ByteOrder, U: MemInt> {
    hwio: HwIoW<'a>,
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
            HwIoR::Mem(ref buf, mask) => {
                U::endian_read_from::<O>(&buf.borrow()[(addr & mask) as usize..])
            }
            HwIoR::Func(ref f) => U::truncate_from(f(addr)),
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
            HwIoW::Mem(ref buf, mask) => {
                U::endian_write_to::<O>(&mut buf.borrow_mut()[(addr & mask) as usize..], val)
            }
            HwIoW::Func(ref f) => f(addr, val.into()),
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

impl<'a: 'b, 'b, 'c: 'b, Order: 'c> Bus<'a, Order>
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
    pub fn fetch_read<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoR<'a, Order, U> {
        self.internal_fetch_read::<U>(addr)
    }

    #[inline(never)]
    pub fn fetch_write<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoW<'a, Order, U> {
        self.internal_fetch_write::<U>(addr)
    }

    #[inline(always)]
    fn internal_fetch_read<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoR<'a, Order, U> {
        self.reads[U::ACCESS_SIZE]
            .lookup(addr)
            .or(Some(self.unmap_r.clone()))
            .unwrap()
            .at(addr)
    }

    #[inline(always)]
    fn internal_fetch_write<U: MemInt + 'b>(&'b self, addr: u32) -> MemIoW<'a, Order, U> {
        self.writes[U::ACCESS_SIZE]
            .lookup(addr)
            .or(Some(self.unmap_w.clone()))
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
            reg.hwio_r::<S>(),
            false,
        )?;
        self.writes[S::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            reg.hwio_w::<S>(),
            false,
        )?;
        Ok(())
    }

    pub fn map_reg8(&mut self, addr: u32, reg: &'a Reg<Order, u8>) -> Result<(), &str> {
        self.mapreg_partial::<u8, u8>(addr, reg)?;
        self.map_combine::<u16>(addr & !1)?;
        self.map_combine::<u32>(addr & !3)?;
        self.map_combine::<u64>(addr & !7)?;
        Ok(())
    }

    pub fn map_reg16<'s, 'r: 's>(
        &'s mut self,
        addr: u32,
        reg: &'a Reg<Order, u16>,
    ) -> Result<(), &'r str> {
        self.mapreg_partial::<u16, u8>(addr, reg)?;
        self.mapreg_partial::<u16, u16>(addr, reg)?;
        self.map_combine::<u32>(addr & !3)?;
        self.map_combine::<u64>(addr & !7)?;
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
        self.map_combine::<u64>(addr & !7)?;
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
            node.insert_range(begin, end, HwIoR::Mem(buf, mask), false)?;
        }
        if !readonly {
            for (_, node) in self.writes.iter_mut() {
                node.insert_range(begin, end, HwIoW::Mem(buf, mask), false)?;
            }
        }

        return Ok(());
    }

    fn map_combine<'r: 'b, U: MemInt + 'a>(&'b mut self, addr: u32) -> Result<(), &'r str> {
        let before = self.fetch_read::<U::Half>(addr);
        let after = self.fetch_read::<U::Half>(addr + (mem::size_of::<U>() as u32) / 2);

        self.reads[U::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            HwIoR::Func(Rc::new(move |_| {
                U::from_halves::<Order>(before.read(), after.read()).into()
            })),
            true, // a combiner might overwrite if already existing
        )?;

        let off: u32 = (mem::size_of::<U>() as u32) / 2;
        let before = self.fetch_write::<U::Half>(addr);
        let after = self.fetch_write::<U::Half>(addr + off);

        self.writes[U::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            HwIoW::Func(Rc::new(move |_, val64| {
                let (mask1, shift1) = Order::subint_mask::<U, U::Half>(0);
                let (mask2, shift2) = Order::subint_mask::<U, U::Half>(off as usize);
                let val_before = (val64 & mask1.into()) >> shift1;
                let val_after = (val64 & mask2.into()) >> shift2;
                before.write(U::Half::truncate_from(val_before));
                after.write(U::Half::truncate_from(val_after));
            })),
            true, // a combiner might overwrite if already existing
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bus::le::{Reg16, Reg32, Reg8, RegFlags};

    extern crate byteorder;
    use self::byteorder::{BigEndian, LittleEndian};

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

        assert_eq!(bus.read::<u64>(0x04000120), 0xaabb56f812345678);
        bus.write::<u64>(0x04000120, 0x0);

        assert_eq!(bus.read::<u32>(0x04000120), 0x00000000);
        assert_eq!(bus.read::<u32>(0x04000124), 0x000056f8);
    }

    #[test]
    fn combiner_le() {
        let reg1 = Reg32::default();
        let reg2 = Reg16::default();
        let reg3 = Reg8::default();
        let reg4 = Reg8::default();

        let mut bus = Bus::<LittleEndian>::new();
        assert_eq!(bus.map_reg32(0xFF000000, &reg1).is_ok(), true);
        assert_eq!(bus.map_reg16(0xFF000004, &reg2).is_ok(), true);
        assert_eq!(bus.map_reg8(0xFF000006, &reg3).is_ok(), true);
        assert_eq!(bus.map_reg8(0xFF000007, &reg4).is_ok(), true);

        bus.write::<u64>(0xFF000000, 0xaabbccdd11223344);
        assert_eq!(reg1.get(), 0x11223344);
        assert_eq!(reg2.get(), 0xccdd);
        assert_eq!(reg3.get(), 0xbb);
        assert_eq!(reg4.get(), 0xaa);
        assert_eq!(bus.read::<u64>(0xFF000000), 0xaabbccdd11223344);
        assert_eq!(bus.read::<u32>(0xFF000000), 0x11223344);
        assert_eq!(bus.read::<u32>(0xFF000004), 0xaabbccdd);
        assert_eq!(bus.read::<u16>(0xFF000004), 0xccdd);
        assert_eq!(bus.read::<u16>(0xFF000006), 0xaabb);
        assert_eq!(bus.read::<u8>(0xFF000006), 0xbb);
        assert_eq!(bus.read::<u8>(0xFF000007), 0xaa);

        bus.write::<u32>(0xFF000004, 0x66778899);
        assert_eq!(bus.read::<u32>(0xFF000004), 0x66778899);
        assert_eq!(bus.read::<u16>(0xFF000004), 0x8899);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x6677);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x77);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x66);

        bus.write::<u16>(0xFF000006, 0x1122);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x1122);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x22);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x11);
    }

    #[test]
    fn combiner_be() {
        use bus::be::{Reg16, Reg32, Reg8};
        let reg1 = Reg32::default();
        let reg2 = Reg16::default();
        let reg3 = Reg8::default();
        let reg4 = Reg8::default();

        let mut bus = Bus::<BigEndian>::new();

        assert_eq!(bus.map_reg32(0xFF000000, &reg1).is_ok(), true);
        assert_eq!(bus.map_reg16(0xFF000004, &reg2).is_ok(), true);
        assert_eq!(bus.map_reg8(0xFF000006, &reg3).is_ok(), true);
        assert_eq!(bus.map_reg8(0xFF000007, &reg4).is_ok(), true);

        bus.write::<u64>(0xFF000000, 0xaabbccdd11223344);
        assert_eq!(reg1.get(), 0xaabbccdd);
        assert_eq!(reg2.get(), 0x1122);
        assert_eq!(reg3.get(), 0x33);
        assert_eq!(reg4.get(), 0x44);
        assert_eq!(bus.read::<u64>(0xFF000000), 0xaabbccdd11223344);
        assert_eq!(bus.read::<u32>(0xFF000000), 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0xFF000004), 0x11223344);
        assert_eq!(bus.read::<u16>(0xFF000004), 0x1122);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x3344);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x33);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x44);

        bus.write::<u32>(0xFF000004, 0x66778899);
        assert_eq!(bus.read::<u32>(0xFF000004), 0x66778899);
        assert_eq!(bus.read::<u16>(0xFF000004), 0x6677);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x8899);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x88);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x99);

        bus.write::<u16>(0xFF000006, 0x1122);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x1122);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x11);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x22);
    }
}
