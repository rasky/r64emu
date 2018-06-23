extern crate byteorder;

use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW};
use super::memint::{ByteOrderCombiner, MemInt};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;

bitflags! {
   pub struct RegFlags: u8 {
        const READACCESS = 0b00000001;
        const WRITEACCESS = 0b00000010;
    }
}

impl RegFlags {
    pub fn new(read: bool, write: bool) -> RegFlags {
        let mut rf = RegFlags::default();
        if !read {
            rf.remove(RegFlags::READACCESS);
        }
        if !write {
            rf.remove(RegFlags::WRITEACCESS);
        }
        return rf;
    }
}

impl Default for RegFlags {
    fn default() -> RegFlags {
        return RegFlags::READACCESS | RegFlags::WRITEACCESS;
    }
}

type Wcb<U> = Option<Rc<Box<Fn(U, U)>>>;
type Rcb<U> = Option<Rc<Box<Fn(U) -> U>>>;

pub struct Reg<O, U>
where
    O: ByteOrderCombiner,
    U: MemInt,
{
    raw: Rc<RefCell<Box<[u8]>>>,
    romask: U,
    flags: RegFlags,
    wcb: Wcb<U>,
    rcb: Rcb<U>,
    phantom: PhantomData<O>,
}

impl<O, U> Default for Reg<O, U>
where
    O: ByteOrderCombiner,
    U: MemInt,
{
    fn default() -> Self {
        Reg {
            raw: Rc::new(RefCell::new(box [0u8; 8])),
            romask: U::zero(),
            flags: RegFlags::default(),
            wcb: None,
            rcb: None,
            phantom: PhantomData,
        }
    }
}

impl<O, U> Reg<O, U>
where
    O: ByteOrderCombiner,
    U: MemInt + 'static,
{
    fn refcell_get(raw: &Rc<RefCell<Box<[u8]>>>) -> U {
        U::endian_read_from::<O>(&raw.borrow()[..])
    }

    fn refcell_set(raw: &Rc<RefCell<Box<[u8]>>>, val: U) {
        U::endian_write_to::<O>(&mut raw.borrow_mut()[..], val)
    }

    pub fn new(init: U, rwmask: U, flags: RegFlags, wcb: Wcb<U>, rcb: Rcb<U>) -> Self {
        let reg = Reg {
            romask: !rwmask,
            flags,
            wcb,
            rcb,
            ..Default::default()
        };
        reg.set(init);
        return reg;
    }

    /// Get the current value of the register in memory, bypassing any callback.
    pub fn get(&self) -> U {
        let val: U = Self::refcell_get(&self.raw);
        val
    }

    /// Set the current value of the register, bypassing any read/write mask or callback.
    pub fn set(&self, val: U) {
        Self::refcell_set(&self.raw, val)
    }

    pub fn hwio_r<S>(&self) -> HwIoR
    where
        S: MemInt + Into<U>, // S is a smaller MemInt type than U
    {
        if !self.flags.contains(RegFlags::READACCESS) {
            return unmapped_area_r();
        }

        match self.rcb {
            Some(ref rcb) => {
                let rcb = Rc::downgrade(&rcb);
                let raw = Rc::downgrade(&self.raw);
                HwIoR::Func(Rc::new(move |addr: u32| {
                    let rcb = rcb.upgrade().unwrap();
                    let raw = raw.upgrade().unwrap();

                    let off = (addr as usize) & (U::SIZE - 1);
                    let (_, shift) = O::subint_mask::<U, S>(off);
                    let real = Self::refcell_get(&raw);
                    let val: u64 = rcb(real).into();
                    S::truncate_from(val >> shift).into()
                }))
            }
            None => HwIoR::Mem(self.raw.clone(), (U::SIZE - 1) as u32),
        }
    }

    pub fn hwio_w<S>(&self) -> HwIoW
    where
        S: MemInt + Into<U>, // S is a smaller MemInt type than U
    {
        if !self.flags.contains(RegFlags::WRITEACCESS) {
            return unmapped_area_w();
        }

        if self.romask == U::zero() && self.wcb.is_none() {
            HwIoW::Mem(self.raw.clone(), (U::SIZE - 1) as u32)
        } else {
            let raw = Rc::downgrade(&self.raw);
            let wcb = self.wcb.clone().map(|f| Rc::downgrade(&f));
            let romask = self.romask;
            HwIoW::Func(Rc::new(move |addr: u32, val64: u64| {
                let raw = raw.upgrade().unwrap();
                let off = (addr as usize) & (U::SIZE - 1);
                let (mut mask, shift) = O::subint_mask::<U, S>(off);
                let mut val = U::truncate_from(val64) << shift;
                let old = Self::refcell_get(&raw);
                mask = !mask | romask;
                val = (val & !mask) | (old & mask);
                Self::refcell_set(&raw, val);
                if let Some(ref f) = wcb {
                    let f = f.upgrade().unwrap();
                    f(old, val);
                }
            }))
        }
    }

    pub fn read<S: MemInt + Into<U>>(&self, addr: u32) -> S {
        self.hwio_r::<S>().at::<O, S>(addr).read()
    }

    pub fn write<S: MemInt + Into<U>>(&self, addr: u32, val: S) {
        self.hwio_w::<S>().at::<O, S>(addr).write(val);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{be, le};
    use super::RegFlags;
    use std::rc::Rc;

    #[test]
    fn reg32le_bare() {
        let r = le::Reg32::default();
        r.set(0xaaaaaaaa);

        r.write::<u32>(0, 0x12345678);
        assert_eq!(r.read::<u8>(0), 0x78);
        assert_eq!(r.read::<u8>(1), 0x56);
        assert_eq!(r.read::<u16>(2), 0x1234);
        r.write::<u16>(0, 0x6789);
        assert_eq!(r.get(), 0x12346789);
    }

    #[test]
    fn reg32be_bare() {
        let r = be::Reg32::default();
        r.set(0xaaaaaaaa);
        r.write::<u32>(0, 0x12345678);
        assert_eq!(r.read::<u8>(0), 0x12);
        assert_eq!(r.read::<u8>(1), 0x34);
        assert_eq!(r.read::<u16>(2), 0x5678);
        r.write::<u16>(0, 0x6789);
        assert_eq!(r.get(), 0x67895678);
    }

    #[test]
    fn reg32le_mask() {
        let r = le::Reg32 {
            romask: 0xff00ff00,
            ..Default::default()
        };
        r.set(0xddccbbaa);
        r.write::<u32>(0, 0x12345678);
        assert_eq!(r.get(), 0xdd34bb78);
        assert_eq!(r.read::<u8>(0), 0x78);
        assert_eq!(r.read::<u8>(1), 0xbb);
        assert_eq!(r.read::<u16>(2), 0xdd34);
        r.write::<u16>(0, 0x6789);
        assert_eq!(r.get(), 0xdd34bb89);
    }

    #[test]
    fn reg32be_mask() {
        let r = be::Reg32 {
            romask: 0xff00ff00,
            ..Default::default()
        };
        r.set(0xddccbbaa);
        r.write::<u32>(0, 0x12345678);
        assert_eq!(r.get(), 0xdd34bb78);
        assert_eq!(r.read::<u8>(0), 0xdd);
        assert_eq!(r.read::<u8>(1), 0x34);
        assert_eq!(r.read::<u16>(2), 0xbb78);
        r.write::<u16>(0, 0x6789);
        assert_eq!(r.get(), 0xdd89bb78);
    }

    #[test]
    fn reg32le_cb() {
        let r = le::Reg32 {
            rcb: Some(Rc::new(box move |val| val | 0x1)),
            ..Default::default()
        };

        r.set(0x12345678);
        assert_eq!(r.read::<u32>(0), 0x12345679);
        r.write::<u16>(0, 0x6788);
        assert_eq!(r.read::<u32>(0), 0x12346789);
        assert_eq!(r.get(), 0x12346788);
    }

    #[test]
    fn reg32le_rowo() {
        let r = le::Reg32 {
            flags: RegFlags::READACCESS,
            ..Default::default()
        };
        r.set(0x12345678);
        assert_eq!(r.read::<u32>(0), 0x12345678);
        r.write::<u32>(0, 0xaabbccdd);
        assert_eq!(r.read::<u32>(0), 0x12345678);

        let r = le::Reg32 {
            flags: RegFlags::WRITEACCESS,
            ..Default::default()
        };
        r.set(0x12345678);
        assert_eq!(r.read::<u32>(0), 0xffffffff);
        r.write::<u32>(0, 0xaabbccdd);
        assert_eq!(r.read::<u32>(0), 0xffffffff);
    }
}
