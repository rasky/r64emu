use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW};
use super::memint::MemInt;

use bitflags::bitflags;
use byteorder::ByteOrder;

use std::cell::{RefCell, RefMut};
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

#[derive(Default)]
pub struct Mem {
    buf: Rc<RefCell<Box<[u8]>>>,
    psize: usize,
    flags: MemFlags,
}

impl Mem {
    pub fn from_buffer(v: Vec<u8>, flags: MemFlags) -> Self {
        let psize = v.len();
        if psize == 0 {
            panic!("bus::mem: memory size cannot be zero")
        }
        if psize & (psize - 1) != 0 {
            panic!("bus::mem: memory size must be pow2")
        }

        Mem {
            buf: Rc::new(RefCell::new(v.into())),
            psize: psize,
            flags: flags,
        }
    }

    pub fn new(psize: usize, flags: MemFlags) -> Self {
        let mut v = Vec::<u8>::new();
        v.resize(psize, 0);
        Self::from_buffer(v, flags)
    }

    pub fn len(&self) -> usize {
        self.psize
    }

    pub fn buf<'a>(&'a self) -> RefMut<'a, Box<[u8]>> {
        self.buf.borrow_mut()
    }

    pub(crate) fn hwio_r<S: MemInt>(&self) -> HwIoR {
        if !self.flags.contains(MemFlags::READACCESS) {
            return unmapped_area_r();
        }

        HwIoR::Mem(self.buf.clone(), (self.psize - 1) as u32)
    }

    pub(crate) fn hwio_w<S: MemInt>(&self) -> HwIoW {
        if !self.flags.contains(MemFlags::WRITEACCESS) {
            return unmapped_area_w();
        }

        HwIoW::Mem(self.buf.clone(), (self.psize - 1) as u32)
    }

    pub fn write<O: ByteOrder, S: MemInt>(&self, addr: u32, val: S) {
        self.hwio_w::<S>().at::<O, S>(addr).write(val);
    }
    pub fn read<O: ByteOrder, S: MemInt>(&self, addr: u32) -> S {
        self.hwio_r::<S>().at::<O, S>(addr).read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_basic() {
        let ram = Mem::new(1024, MemFlags::default());

        for x in &mut ram.buf()[128..130] {
            *x = 5;
        }

        assert_eq!(ram.buf()[128], 5);
    }
}
