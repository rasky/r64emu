extern crate byteorder;

use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW};
use super::memint::{ByteOrderCombiner, MemInt};
use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
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
    name: String,
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
            name: String::new(),
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

    pub fn new(name: &str, init: U, rwmask: U, flags: RegFlags, wcb: Wcb<U>, rcb: Rcb<U>) -> Self {
        let reg = Reg {
            name: name.into(),
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

    pub(crate) fn hwio_r<S>(&self) -> HwIoR
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

    pub(crate) fn hwio_w<S>(&self) -> HwIoW
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
}

impl<O: ByteOrderCombiner, U: MemInt + 'static> fmt::Debug for Reg<O, U> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let name = if self.name.is_empty() {
            "Reg"
        } else {
            &self.name
        };
        let val: u64 = self.get().into();
        let rwmask: u64 = (!self.romask).into();
        fmt.debug_struct(name)
            .field("val", &format!("0x{:x}", val))
            .field("rwmask", &format!("0x{:x}", &rwmask))
            .field("flags", &self.flags)
            .field("rcb", &self.rcb.is_some())
            .field("wcb", &self.wcb.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::super::memint::{ByteOrderCombiner, MemInt};
    use super::super::{be, le};
    use super::{Reg, RegFlags};
    use std::marker::PhantomData;
    use std::rc::Rc;

    #[derive(Default)]
    struct FakeBus<O: ByteOrderCombiner, U: MemInt + 'static> {
        phantom: PhantomData<(O, U)>,
    }

    impl<O: ByteOrderCombiner, U: MemInt + 'static> FakeBus<O, U> {
        fn read<S: MemInt + Into<U>>(&self, reg: &Reg<O, U>, addr: u32) -> S {
            reg.hwio_r::<S>().at::<O, S>(addr).read()
        }

        fn write<S: MemInt + Into<U>>(&self, reg: &Reg<O, U>, addr: u32, val: S) {
            reg.hwio_w::<S>().at::<O, S>(addr).write(val);
        }
    }

    #[test]
    fn reg32le_bare() {
        let bus = FakeBus::default();
        let r = le::Reg32::default();
        r.set(0xaaaaaaaa);

        bus.write::<u32>(&r, 0, 0x12345678);
        assert_eq!(bus.read::<u8>(&r, 0), 0x78);
        assert_eq!(bus.read::<u8>(&r, 1), 0x56);
        assert_eq!(bus.read::<u16>(&r, 2), 0x1234);
        bus.write::<u16>(&r, 0, 0x6789);
        assert_eq!(r.get(), 0x12346789);
    }

    #[test]
    fn reg32be_bare() {
        let bus = FakeBus::default();
        let r = be::Reg32::default();
        r.set(0xaaaaaaaa);
        bus.write::<u32>(&r, 0, 0x12345678);
        assert_eq!(bus.read::<u8>(&r, 0), 0x12);
        assert_eq!(bus.read::<u8>(&r, 1), 0x34);
        assert_eq!(bus.read::<u16>(&r, 2), 0x5678);
        bus.write::<u16>(&r, 0, 0x6789);
        assert_eq!(r.get(), 0x67895678);
    }

    #[test]
    fn reg32le_mask() {
        let bus = FakeBus::default();
        let r = le::Reg32 {
            romask: 0xff00ff00,
            ..Default::default()
        };
        r.set(0xddccbbaa);
        bus.write::<u32>(&r, 0, 0x12345678);
        assert_eq!(r.get(), 0xdd34bb78);
        assert_eq!(bus.read::<u8>(&r, 0), 0x78);
        assert_eq!(bus.read::<u8>(&r, 1), 0xbb);
        assert_eq!(bus.read::<u16>(&r, 2), 0xdd34);
        bus.write::<u16>(&r, 0, 0x6789);
        assert_eq!(r.get(), 0xdd34bb89);
    }

    #[test]
    fn reg32be_mask() {
        let bus = FakeBus::default();
        let r = be::Reg32 {
            romask: 0xff00ff00,
            ..Default::default()
        };
        r.set(0xddccbbaa);
        bus.write::<u32>(&r, 0, 0x12345678);
        assert_eq!(r.get(), 0xdd34bb78);
        assert_eq!(bus.read::<u8>(&r, 0), 0xdd);
        assert_eq!(bus.read::<u8>(&r, 1), 0x34);
        assert_eq!(bus.read::<u16>(&r, 2), 0xbb78);
        bus.write::<u16>(&r, 0, 0x6789);
        assert_eq!(r.get(), 0xdd89bb78);
    }

    #[test]
    fn reg32le_cb() {
        let bus = FakeBus::default();
        let r = le::Reg32 {
            rcb: Some(Rc::new(box move |val| val | 0x1)),
            ..Default::default()
        };

        r.set(0x12345678);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12345679);
        bus.write::<u16>(&r, 0, 0x6788);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12346789);
        assert_eq!(r.get(), 0x12346788);
    }

    #[test]
    fn reg32le_rowo() {
        let bus = FakeBus::default();
        let r = le::Reg32 {
            flags: RegFlags::READACCESS,
            ..Default::default()
        };
        r.set(0x12345678);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12345678);
        bus.write::<u32>(&r, 0, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12345678);

        let r = le::Reg32 {
            flags: RegFlags::WRITEACCESS,
            ..Default::default()
        };
        r.set(0x12345678);
        assert_eq!(bus.read::<u32>(&r, 0), 0xffffffff);
        bus.write::<u32>(&r, 0, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(&r, 0), 0xffffffff);
    }
}
