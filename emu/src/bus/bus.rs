extern crate byteorder;

use self::byteorder::ByteOrder;
use super::device::{DevPtr, Device};
use super::mem::{Mem, MemFlags};
use super::memint::{AccessSize, ByteOrderCombiner, MemInt};
use super::radix::RadixTree;
use super::regs::Reg;
use enum_map::EnumMap;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;

#[derive(Clone)]
pub enum HwIoR {
    Mem(Rc<RefCell<Box<[u8]>>>, u32),
    Func(Rc<Fn(u32) -> u64>),
}

#[derive(Clone)]
pub enum HwIoW {
    Mem(Rc<RefCell<Box<[u8]>>>, u32),
    Func(Rc<Fn(u32, u64)>),
}

impl HwIoR {
    pub fn at<O: ByteOrder, U: MemInt>(&self, addr: u32) -> MemIoR<O, U> {
        MemIoR {
            hwio: self.clone(),
            addr,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn read<O: ByteOrder, U: MemInt>(&self, addr: u32) -> U {
        match self {
            HwIoR::Mem(buf, mask) => {
                U::endian_read_from::<O>(&buf.borrow()[(addr & mask) as usize..])
            }
            HwIoR::Func(f) => U::truncate_from(f(addr)),
        }
    }
}

impl HwIoW {
    pub fn at<O: ByteOrder, U: MemInt>(&self, addr: u32) -> MemIoW<O, U> {
        MemIoW {
            hwio: self.clone(),
            addr,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn write<O: ByteOrder, U: MemInt>(&self, addr: u32, val: U) {
        match self {
            HwIoW::Mem(buf, mask) => {
                U::endian_write_to::<O>(&mut buf.borrow_mut()[(addr & mask) as usize..], val)
            }
            HwIoW::Func(f) => f(addr, val.into()),
        }
    }
}

pub struct MemIoR<O: ByteOrder, U: MemInt> {
    hwio: HwIoR,
    addr: u32,
    phantom: PhantomData<(O, U)>,
}

pub struct MemIoW<O: ByteOrder, U: MemInt> {
    hwio: HwIoW,
    addr: u32,
    phantom: PhantomData<(O, U)>,
}

impl<'a, O: ByteOrder, U: MemInt> MemIoR<O, U> {
    pub fn read(&self) -> U {
        self.hwio.read::<O, U>(self.addr)
    }
}

impl<'a, O: ByteOrder, U: MemInt> MemIoW<O, U> {
    pub fn write(&self, val: U) {
        self.hwio.write::<O, U>(self.addr, val);
    }
}

pub fn unmapped_area_r() -> HwIoR {
    thread_local!(
        static FN: Rc<Fn(u32)->u64> = Rc::new(|_| {
            // FIXME: log
            return 0xffffffffffffffff;
        })
    );
    HwIoR::Func(FN.with(|c| c.clone()))
}

pub fn unmapped_area_w() -> HwIoW {
    thread_local!(
        static FN: Rc<Fn(u32,u64)> = Rc::new(|_,_| {
            // FIXME: log
        })
    );
    HwIoW::Func(FN.with(|c| c.clone()))
}

pub struct Bus<'a, Order: ByteOrderCombiner + 'a> {
    reads: EnumMap<AccessSize, Box<RadixTree<HwIoR>>>,
    writes: EnumMap<AccessSize, Box<RadixTree<HwIoW>>>,

    unmap_r: HwIoR,
    unmap_w: HwIoW,

    phantom: PhantomData<&'a Order>,
}

impl<'a: 'b, 'b, 's: 'b, Order> Bus<'a, Order>
where
    Order: ByteOrderCombiner + 'static,
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

    pub fn read<U: MemInt + 'a>(&'b self, addr: u32) -> U {
        self.internal_fetch_read::<U>(addr).read::<Order, U>(addr)
    }

    pub fn write<U: MemInt + 'a>(&'b mut self, addr: u32, val: U) {
        self.internal_fetch_write::<U>(addr)
            .write::<Order, U>(addr, val);
    }

    #[inline(never)]
    pub fn fetch_read<U: MemInt + 'a>(&'b self, addr: u32) -> MemIoR<Order, U> {
        self.internal_fetch_read::<U>(addr).at(addr)
    }

    #[inline(never)]
    pub fn fetch_write<U: MemInt + 'a>(&'b self, addr: u32) -> MemIoW<Order, U> {
        self.internal_fetch_write::<U>(addr).at(addr)
    }

    #[inline(always)]
    fn internal_fetch_read<U: MemInt + 'a>(&'b self, addr: u32) -> &'b HwIoR {
        self.reads[U::ACCESS_SIZE]
            .lookup(addr)
            .or(Some(&self.unmap_r))
            .unwrap()
    }

    #[inline(always)]
    fn internal_fetch_write<U: MemInt + 'a>(&'b self, addr: u32) -> &'b HwIoW {
        self.writes[U::ACCESS_SIZE]
            .lookup(addr)
            .or(Some(&self.unmap_w))
            .unwrap()
    }

    fn mapreg_partial<U: 'static, S>(
        &'b mut self,
        addr: u32,
        reg: &'b Reg<Order, U>,
    ) -> Result<(), &'s str>
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

    pub fn map_reg8(&'b mut self, addr: u32, reg: &'a Reg<Order, u8>) -> Result<(), &'s str> {
        self.mapreg_partial::<u8, u8>(addr, reg)?;
        self.map_combine::<u16>(addr & !1)?;
        self.map_combine::<u32>(addr & !3)?;
        self.map_combine::<u64>(addr & !7)?;
        Ok(())
    }

    pub fn map_reg16(&'b mut self, addr: u32, reg: &'b Reg<Order, u16>) -> Result<(), &'s str> {
        self.mapreg_partial::<u16, u8>(addr, reg)?;
        self.mapreg_partial::<u16, u16>(addr, reg)?;
        self.map_combine::<u32>(addr & !3)?;
        self.map_combine::<u64>(addr & !7)?;
        Ok(())
    }

    pub fn map_reg32(&'b mut self, addr: u32, reg: &'b Reg<Order, u32>) -> Result<(), &'s str> {
        self.mapreg_partial::<u32, u8>(addr, reg)?;
        self.mapreg_partial::<u32, u16>(addr, reg)?;
        self.mapreg_partial::<u32, u32>(addr, reg)?;
        self.map_combine::<u64>(addr & !7)?;
        Ok(())
    }

    pub fn map_reg64(&'b mut self, addr: u32, reg: &'b Reg<Order, u64>) -> Result<(), &'s str> {
        self.mapreg_partial::<u64, u8>(addr, reg)?;
        self.mapreg_partial::<u64, u16>(addr, reg)?;
        self.mapreg_partial::<u64, u32>(addr, reg)?;
        self.mapreg_partial::<u64, u64>(addr, reg)?;
        Ok(())
    }

    pub fn map_mem(&'b mut self, begin: u32, end: u32, mem: &'b Mem) -> Result<(), &'s str> {
        self.reads[AccessSize::Size8].insert_range(begin, end, mem.hwio_r::<u8>(), false)?;
        self.reads[AccessSize::Size16].insert_range(begin, end, mem.hwio_r::<u16>(), false)?;
        self.reads[AccessSize::Size32].insert_range(begin, end, mem.hwio_r::<u32>(), false)?;
        self.reads[AccessSize::Size64].insert_range(begin, end, mem.hwio_r::<u64>(), false)?;

        self.writes[AccessSize::Size8].insert_range(begin, end, mem.hwio_w::<u8>(), false)?;
        self.writes[AccessSize::Size16].insert_range(begin, end, mem.hwio_w::<u16>(), false)?;
        self.writes[AccessSize::Size32].insert_range(begin, end, mem.hwio_w::<u32>(), false)?;
        self.writes[AccessSize::Size64].insert_range(begin, end, mem.hwio_w::<u64>(), false)?;

        return Ok(());
    }

    pub fn map_device<T>(&'b mut self, device: &mut DevPtr<T>) -> Result<(), &'s str>
    where
        T: Device<Order = Order>,
    {
        device.borrow_mut().dev_map(self)
    }

    fn map_combine<U: MemInt + 'static>(&'b mut self, addr: u32) -> Result<(), &'s str> {
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
        let ram1 = Mem::new(1024, MemFlags::default());
        let mut bus = Bus::<LittleEndian>::new();

        assert_eq!(bus.map_mem(0x04000000, 0x06000000, &ram1).is_ok(), true);
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
            Some(Rc::new(box |x| x | 0xf0)),
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
