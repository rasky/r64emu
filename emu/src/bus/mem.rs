use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW};
use crate::memint::{AccessSize, MemInt};
use crate::state::ArrayField;

use bitflags::bitflags;
use byteorder::ByteOrder;

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

bitflags! {
   pub struct MemFlags: u8 {
        const READACCESS = 0b00000001;
        const WRITEACCESS = 0b00000010;
    }
}

impl MemFlags {
    pub fn new(read: bool, write: bool) -> MemFlags {
        let mut flags = MemFlags::default();
        if !read {
            flags.remove(MemFlags::READACCESS);
        }
        if !write {
            flags.remove(MemFlags::WRITEACCESS);
        }
        return flags;
    }
}

impl Default for MemFlags {
    fn default() -> MemFlags {
        return MemFlags::READACCESS | MemFlags::WRITEACCESS;
    }
}

type Wcb = Rc<Box<Fn(u32, AccessSize, u64, u64)>>;

#[derive(Default)]
pub struct Mem {
    name: &'static str,
    buf: ArrayField<u8>,
    mask: u32,
    flags: MemFlags,
    wcb: Option<Wcb>,
}

impl Mem {
    pub fn from_buffer(name: &'static str, v: Vec<u8>, flags: MemFlags) -> Self {
        let mut mem = Self::new(name, v.len(), flags, None);
        mem.copy_from_slice(&v[..]);
        mem
    }

    pub fn new(name: &'static str, psize: usize, flags: MemFlags, wcb: Option<Wcb>) -> Self {
        // Avoid serializing ROMs into save state
        let serialized = flags.contains(MemFlags::WRITEACCESS);
        Self {
            name,
            buf: ArrayField::internal_new(name, 0, psize, serialized),
            flags: flags,
            mask: psize.next_power_of_two() as u32 - 1,
            wcb,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn can_read(&self) -> bool {
        self.flags.contains(MemFlags::READACCESS)
    }
    pub fn can_write(&self) -> bool {
        self.flags.contains(MemFlags::WRITEACCESS)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub(crate) fn hwio_r<S: MemInt>(&self) -> HwIoR {
        if self.name == "" {
            panic!("uninitialized Mem in hwio_r");
        }
        if !self.flags.contains(MemFlags::READACCESS) {
            return unmapped_area_r();
        }

        HwIoR::Mem(unsafe { self.buf.clone() }, self.mask)
    }

    pub(crate) fn hwio_w<O: ByteOrder, S: MemInt>(&self) -> HwIoW {
        if self.name == "" {
            panic!("uninitialized Mem in hwio_w");
        }
        if !self.flags.contains(MemFlags::WRITEACCESS) {
            return unmapped_area_w();
        }

        match self.wcb {
            Some(ref wcb) => {
                let wcb = wcb.clone();
                let mut buf = unsafe { self.buf.clone() };
                let mask = self.mask;
                HwIoW::Func(Rc::new(RefCell::new(move |addr: u32, val64: u64| {
                    let mut mem = &mut buf[(addr & mask) as usize..];
                    let val = S::truncate_from(val64);
                    let old = S::endian_read_from::<O>(&mem);
                    S::endian_write_to::<O>(&mut mem, val);
                    (*wcb)(addr, S::ACCESS_SIZE, old.into(), val.into());
                })))
            }
            None => HwIoW::Mem(unsafe { self.buf.clone() }, self.mask),
        }
    }

    pub fn write<O: ByteOrder, S: MemInt>(&self, addr: u32, val: S) {
        self.hwio_w::<O, S>().at::<O, S>(addr).write(val);
    }
    pub fn read<O: ByteOrder, S: MemInt>(&self, addr: u32) -> S {
        self.hwio_r::<S>().at::<O, S>(addr).read()
    }
}

impl Deref for Mem {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl DerefMut for Mem {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::BigEndian;

    #[test]
    fn mem_basic() {
        let mut ram = Mem::new("basic", 1024, MemFlags::default(), None);

        for x in &mut ram[128..130] {
            *x = 5;
        }

        assert_eq!(ram[128], 5);
    }

    #[test]
    fn mem_callback() {
        let called = Rc::new(RefCell::new(Vec::new()));

        let called2 = called.clone();
        let ram = Mem::new(
            "basic",
            1024,
            MemFlags::default(),
            Some(Rc::new(Box::new(move |addr, size, old, new| {
                called2.borrow_mut().push((addr, size, old, new));
            }))),
        );

        ram.write::<BigEndian, u64>(0xFF000080, 0x11223344_55667788);
        ram.write::<BigEndian, u32>(0xFF000082, 0xAABBCCDD);
        ram.write::<BigEndian, u16>(0xFF000086, 0xEEFF);
        ram.write::<BigEndian, u8>(0xFF000083, 0x00);

        assert_eq!(
            &*called.borrow(),
            &vec![
                (0xFF000080, AccessSize::Size64, 0, 0x11223344_55667788),
                (0xFF000082, AccessSize::Size32, 0x33445566, 0xAABBCCDD),
                (0xFF000086, AccessSize::Size16, 0x7788, 0xEEFF),
                (0xFF000083, AccessSize::Size8, 0xBB, 0x00),
            ]
        );

        assert_eq!(ram.read::<BigEndian, u64>(0xEE000080), 0x1122AA00_CCDDEEFF);
    }
}
