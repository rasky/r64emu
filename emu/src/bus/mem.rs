use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW};
use crate::memint::MemInt;
use crate::state::ArrayField;

use bitflags::bitflags;
use byteorder::ByteOrder;

use std::ops::{Deref, DerefMut};

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
    name: &'static str,
    buf: ArrayField<u8>,
    mask: usize,
    flags: MemFlags,
}

impl Mem {
    pub fn from_buffer(name: &'static str, v: Vec<u8>, flags: MemFlags) -> Self {
        let mut mem = Self::new(name, v.len(), flags);
        mem.copy_from_slice(&v[..]);
        mem
    }

    pub fn new(name: &'static str, psize: usize, flags: MemFlags) -> Self {
        // Avoid serializing ROMs into save state
        let serialized = flags.contains(MemFlags::WRITEACCESS);
        Self {
            name,
            buf: ArrayField::new(name, 0, psize, serialized),
            flags: flags,
            mask: psize.next_power_of_two() - 1,
        }
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

        HwIoR::Mem(self.buf.clone(), self.mask as u32)
    }

    pub(crate) fn hwio_w<S: MemInt>(&self) -> HwIoW {
        if self.name == "" {
            panic!("uninitialized Mem in hwio_w");
        }
        if !self.flags.contains(MemFlags::WRITEACCESS) {
            return unmapped_area_w();
        }

        HwIoW::Mem(self.buf.clone(), self.mask as u32)
    }

    pub fn write<O: ByteOrder, S: MemInt>(&self, addr: u32, val: S) {
        self.hwio_w::<S>().at::<O, S>(addr).write(val);
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

    #[test]
    fn mem_basic() {
        let mut ram = Mem::new("basic", 1024, MemFlags::default());

        for x in &mut ram[128..130] {
            *x = 5;
        }

        assert_eq!(ram[128], 5);
    }
}
