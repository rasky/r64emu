extern crate byteorder;

use super::bus::{MemIoR, MemIoW, RawPtr, RawPtrMut};
use super::memint::{MemInt,ByteOrderCombiner};
use std::mem::size_of;
use std::cell::RefCell;
use std::marker::PhantomData;

trait Register {
    type U: MemInt;
}

trait RegBank {
    fn get_regs<'a, U: MemInt>(&'a self) -> Vec<&(Register<U = U> + 'a)>;
}

bitflags! {
    struct RegFlags: u8 {
        const READACCESS = 0b00000001;
        const WRITEACCESS = 0b00000010;
    }
}

impl Default for RegFlags {
    fn default() -> RegFlags { return RegFlags::READACCESS|RegFlags::WRITEACCESS }
}

#[derive(Default)]
pub struct Reg<'a,O,U>
where
    O: ByteOrderCombiner,
    U: MemInt,
{
    raw: RefCell<[u8; 8]>,
    romask: U,
    flags: RegFlags,
    wcb: Option<Box<'a + FnMut(U, U)>>,
    rcb: Option<Box<'a + Fn(U) -> U>>,
    phantom: PhantomData<O>,
}

impl<'a,O,U> Reg<'a,O,U>
where
    O: ByteOrderCombiner,
    U: MemInt,
{
    fn new() -> Self {
        Default::default()
    }

    /// Get the current value of the register in memory, bypassing any callback.
    pub fn get(&self) -> U {
        U::endian_read_from::<O>(&self.raw.borrow()[..])
    }

    /// Set the current value of the register, bypassing any read/write mask or callback.
    pub fn set(&self, val: U) {
        U::endian_write_to::<O>(&mut self.raw.borrow_mut()[..], val)
    }

    fn mem_io_r<S>(&self, addr: u32) -> MemIoR<O, S>
    where
        S: MemInt+Into<U>    // S is a smaller MemInt type than U
    {
        if !self.flags.contains(RegFlags::READACCESS) {
            return MemIoR::Unmapped(PhantomData,PhantomData);
        }

        let off = (addr as usize) & (U::SIZE-1);
        match self.rcb {
            Some(ref f) => MemIoR::Func(Box::new(move || {
                let (_,shift) = O::subint_mask::<U,S>(off);
                let val : u64 = f(self.get()).into();
                S::truncate_from(val >> shift).into()
            })),
            None => MemIoR::Raw(RawPtr(&self.raw.borrow()[off])),
        }
    }

    fn mem_io_w<S>(&mut self, addr: u32) -> MemIoW<O, S>
    where
        S: MemInt+Into<U>    // S is a smaller MemInt type than U
    {
        if !self.flags.contains(RegFlags::WRITEACCESS) {
            return MemIoW::Unmapped(PhantomData,PhantomData);
        }

        let off = (addr as usize) & (U::SIZE-1);
        if self.romask == U::zero() && self.wcb.is_none() {
            MemIoW::Raw(RawPtrMut(&mut self.raw.borrow_mut()[off]))
        } else {
            MemIoW::Func(Box::new(move |val64| {
                let (mut mask,shift) = O::subint_mask::<U,S>(off);
                let mut val = U::truncate_from(val64) << shift;
                let old = self.get();
                mask = !mask | self.romask;
                val = (val & !mask) | (old & mask);
                self.set(val);
                if let Some(ref mut f) = self.wcb {
                    f(old, val);
                }
            }))
        }
    }
}

pub mod le {
    use super::byteorder::LittleEndian;
    pub type Reg8<'a> = super::Reg<'a, LittleEndian, u8>;
    pub type Reg16<'a> = super::Reg<'a, LittleEndian, u16>;
    pub type Reg32<'a> = super::Reg<'a, LittleEndian, u32>;
    pub type Reg64<'a> = super::Reg<'a, LittleEndian, u64>;
}

pub mod be {
    use super::byteorder::BigEndian;
    pub type Reg8<'a> = super::Reg<'a, BigEndian, u8>;
    pub type Reg16<'a> = super::Reg<'a, BigEndian, u16>;
    pub type Reg32<'a> = super::Reg<'a, BigEndian, u32>;
    pub type Reg64<'a> = super::Reg<'a, BigEndian, u64>;
}


#[cfg(test)]
mod tests {
    use super::{le,be,RegFlags};
    use std::cell::RefCell;

    #[test]
    fn reg32le_bare() {
        let mut r = le::Reg32::new();
        r.set(0xaaaaaaaa);
        r.mem_io_w::<u32>(0).write(0x12345678);
        assert_eq!(r.mem_io_r::<u8>(0).read(), 0x78);
        assert_eq!(r.mem_io_r::<u8>(1).read(), 0x56);
        assert_eq!(r.mem_io_r::<u16>(2).read(), 0x1234);
        r.mem_io_w::<u16>(0).write(0x6789);
        assert_eq!(r.get(), 0x12346789);
    }

    #[test]
    fn reg32be_bare() {
        let mut r = be::Reg32::new();
        r.set(0xaaaaaaaa);
        r.mem_io_w::<u32>(0).write(0x12345678);
        assert_eq!(r.mem_io_r::<u8>(0).read(), 0x12);
        assert_eq!(r.mem_io_r::<u8>(1).read(), 0x34);
        assert_eq!(r.mem_io_r::<u16>(2).read(), 0x5678);
        r.mem_io_w::<u16>(0).write(0x6789);
        assert_eq!(r.get(), 0x67895678);
    }

    #[test]
    fn reg32le_mask() {
        let mut r = le::Reg32{romask:0xff00ff00, ..Default::default()};
        r.set(0xddccbbaa);
        r.mem_io_w::<u32>(0).write(0x12345678);
        assert_eq!(r.get(), 0xdd34bb78);
        assert_eq!(r.mem_io_r::<u8>(0).read(), 0x78);
        assert_eq!(r.mem_io_r::<u8>(1).read(), 0xbb);
        assert_eq!(r.mem_io_r::<u16>(2).read(), 0xdd34);
        r.mem_io_w::<u16>(0).write(0x6789);
        assert_eq!(r.get(), 0xdd34bb89);
    }

    #[test]
    fn reg32be_mask() {
        let mut r = be::Reg32{romask:0xff00ff00, ..Default::default()};
        r.set(0xddccbbaa);
        r.mem_io_w::<u32>(0).write(0x12345678);
        assert_eq!(r.get(), 0xdd34bb78);
        assert_eq!(r.mem_io_r::<u8>(0).read(), 0xdd);
        assert_eq!(r.mem_io_r::<u8>(1).read(), 0x34);
        assert_eq!(r.mem_io_r::<u16>(2).read(), 0xbb78);
        r.mem_io_w::<u16>(0).write(0x6789);
        assert_eq!(r.get(), 0xdd89bb78);
    }

    #[test]
    fn reg32le_cb() {
        let mut r = le::Reg32{
            rcb: Some(box |val| {
                val | 0x1
            }),
            ..Default::default()};

        r.set(0x12345678);
        assert_eq!(r.mem_io_r::<u32>(0).read(), 0x12345679);
        r.mem_io_w::<u16>(0).write(0x6788);
        assert_eq!(r.mem_io_r::<u32>(0).read(), 0x12346789);
        assert_eq!(r.get(), 0x12346788);
    }

    #[test]
    fn reg32le_rowo() {
        let mut r = le::Reg32{flags:RegFlags::READACCESS,..Default::default()};
        r.set(0x12345678);
        assert_eq!(r.mem_io_r::<u32>(0).read(), 0x12345678);
        r.mem_io_w::<u32>(0).write(0xaabbccdd);
        assert_eq!(r.mem_io_r::<u32>(0).read(), 0x12345678);

        let mut r = le::Reg32{flags:RegFlags::WRITEACCESS,..Default::default()};
        r.set(0x12345678);
        assert_eq!(r.mem_io_r::<u32>(0).read(), 0xffffffff);
        r.mem_io_w::<u32>(0).write(0xaabbccdd);
        assert_eq!(r.mem_io_r::<u32>(0).read(), 0xffffffff);
    }
}
