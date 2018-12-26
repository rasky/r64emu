use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW, MemIoR, MemIoW};
use crate::memint::{ByteOrderCombiner, MemInt};
use crate::state::EndianField;

use bitflags::bitflags;

use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
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

#[derive(Default)]
pub struct Reg<O, U>
where
    O: ByteOrderCombiner,
    U: MemInt,
{
    name: &'static str,     // Name
    raw: EndianField<U, O>, // State field (in the specified endianess). See set/get
    romask: U,              // Mask of read-only bits
    flags: RegFlags,        // Flags
    wcb: Wcb<U>,            // Optional callback invoked when register is written
    rcb: Rcb<U>,            // Optional callback invoked when register is read
    phantom: PhantomData<O>,
}

impl<O, U> Reg<O, U>
where
    O: ByteOrderCombiner + 'static,
    U: MemInt + 'static,
{
    pub fn new(
        name: &'static str,
        init: U,
        rwmask: U,
        flags: RegFlags,
        wcb: Wcb<U>,
        rcb: Rcb<U>,
    ) -> Self {
        Self {
            name: name,
            raw: EndianField::new(name, init),
            romask: !rwmask,
            flags,
            wcb,
            rcb,
            ..Default::default()
        }
    }

    pub fn new_basic(name: &'static str) -> Self {
        Reg::new(
            name,
            U::zero(),
            U::max_value(),
            RegFlags::default(),
            None,
            None,
        )
    }
    pub fn with_rwmask(mut self, rwmask: U) -> Self {
        self.romask = !rwmask;
        self
    }
    pub fn with_flags(mut self, flags: RegFlags) -> Self {
        self.flags = flags;
        self
    }
    pub fn with_wcb(mut self, wcb: Wcb<U>) -> Self {
        self.wcb = wcb;
        self
    }
    pub fn with_rcb(mut self, rcb: Rcb<U>) -> Self {
        self.rcb = rcb;
        self
    }

    pub fn as_ref<R: RegDeref<Type = U>>(&self) -> RegRef<O, R> {
        RegRef::new(self)
    }

    /// Get the current value of the register in memory, bypassing any callback.
    /// Note that the value stored into the State Field is in the order
    /// specified by O. This is counter-intuitive (as)
    pub fn get(&self) -> U {
        self.raw.get()
    }

    /// Set the current value of the register, bypassing any read/write mask or callback.
    pub fn set(&mut self, val: U) {
        self.raw.set(val);
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
                let raw = self.raw.clone();
                HwIoR::Func(Rc::new(move |addr: u32| {
                    let rcb = rcb.upgrade().unwrap();

                    let off = (addr as usize) & (U::SIZE - 1);
                    let (_, shift) = O::subint_mask::<U, S>(off);
                    let val: u64 = rcb(raw.get()).into();
                    S::truncate_from(val >> shift).into()
                }))
            }
            None => HwIoR::Mem(self.raw.as_array_field(), (U::SIZE - 1) as u32),
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
            HwIoW::Mem(self.raw.as_array_field(), (U::SIZE - 1) as u32)
        } else {
            let mut raw = self.raw.clone();
            let wcb = self.wcb.clone().map(|f| Rc::downgrade(&f));
            let romask = self.romask;
            HwIoW::Func(Rc::new(RefCell::new(move |addr: u32, val64: u64| {
                let off = (addr as usize) & (U::SIZE - 1);
                let (mut mask, shift) = O::subint_mask::<U, S>(off);
                let mut val = U::truncate_from(val64) << shift;
                let old = raw.get();
                mask = !mask | romask;
                val = (val & !mask) | (old & mask);
                raw.set(val);
                if let Some(ref f) = wcb {
                    let f = f.upgrade().unwrap();
                    f(old, val);
                }
            })))
        }
    }

    pub fn reader(&self) -> MemIoR<O, U> {
        self.hwio_r::<U>().at::<O, U>(0)
    }

    pub fn writer(&self) -> MemIoW<O, U> {
        self.hwio_w::<U>().at::<O, U>(0)
    }
}

impl<O: ByteOrderCombiner + 'static, U: MemInt + 'static> fmt::Debug for Reg<O, U> {
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

/// A type that can be used as reference to a register (eg: a bitflags).
/// Implementing this trait allows to use Reg::as_ref as a convenient way
/// to access a reference to a register, with conversion and scoping.
pub trait RegDeref {
    type Type: MemInt + 'static;
    fn from(v: Self::Type) -> Self;
    fn to(&self) -> Self::Type;
}

impl<U: MemInt + 'static> RegDeref for U {
    type Type = U;
    fn from(v: Self::Type) -> Self {
        v
    }
    fn to(&self) -> Self::Type {
        *self
    }
}

/// A scoped reference to a Reg.
pub struct RegRef<O: 'static + ByteOrderCombiner, U: RegDeref> {
    raw: EndianField<U::Type, O>,
    val: U,
    old: U,
    phantom: PhantomData<(O, U)>,
}

impl<O: ByteOrderCombiner + 'static, U: RegDeref> RegRef<O, U> {
    fn new(r: &Reg<O, U::Type>) -> Self {
        let val = r.get();
        Self {
            raw: r.raw.clone(),
            val: U::from(val),
            old: U::from(val),
            phantom: PhantomData,
        }
    }
}

impl<O: 'static + ByteOrderCombiner, U: RegDeref> Drop for RegRef<O, U> {
    fn drop(&mut self) {
        let val = self.val.to();
        let old = self.old.to();
        if val != old {
            self.raw.set(val);
        }
    }
}

impl<O: ByteOrderCombiner, U: RegDeref> Deref for RegRef<O, U> {
    type Target = U;
    fn deref(&self) -> &U {
        &self.val
    }
}

impl<O: ByteOrderCombiner, U: RegDeref> DerefMut for RegRef<O, U> {
    fn deref_mut(&mut self) -> &mut U {
        &mut self.val
    }
}

#[cfg(test)]
mod tests {
    use super::super::{be, le};
    use super::{Reg, RegFlags};
    use crate::memint::{ByteOrderCombiner, MemInt};
    use std::marker::PhantomData;
    use std::rc::Rc;

    #[derive(Default)]
    struct FakeBus<O: ByteOrderCombiner, U: MemInt + 'static> {
        phantom: PhantomData<(O, U)>,
    }

    impl<O: ByteOrderCombiner + 'static, U: MemInt + 'static> FakeBus<O, U> {
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
        let mut r = le::Reg32::new_basic("r1");
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
        let mut r = be::Reg32::new_basic("r2");
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
        let mut r = le::Reg32::new_basic("reg32").with_rwmask(0x00ff00ff);
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
        let mut r = be::Reg32::new_basic("reg32").with_rwmask(0x00ff00ff);
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
        let mut r = le::Reg32::new_basic("reg32").with_rcb(Some(Rc::new(box move |val| val | 0x1)));
        r.set(0x12345678);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12345679);
        bus.write::<u16>(&r, 0, 0x6788);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12346789);
        assert_eq!(r.get(), 0x12346788);
    }

    #[test]
    fn reg32le_rowo() {
        let bus = FakeBus::default();
        let mut r = le::Reg32::new_basic("reg32ra").with_flags(RegFlags::READACCESS);
        r.set(0x12345678);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12345678);
        bus.write::<u32>(&r, 0, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(&r, 0), 0x12345678);

        let mut r = le::Reg32::new_basic("reg32wa").with_flags(RegFlags::WRITEACCESS);
        r.set(0x12345678);
        assert_eq!(bus.read::<u32>(&r, 0), 0xffffffff);
        bus.write::<u32>(&r, 0, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(&r, 0), 0xffffffff);
    }
}
