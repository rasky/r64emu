extern crate byteorder;

use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW};
use super::memint::MemInt;
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
    pub fn new(psize: usize, flags: MemFlags) -> Self {
        if psize & (psize - 1) != 0 {
            panic!("bus::mem: psize must be pow2")
        }

        let mut v = Vec::<u8>::new();
        v.resize(psize, 0);

        Mem {
            buf: Rc::new(RefCell::new(v.into())),
            psize: psize,
            flags: flags,
        }
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
