extern crate byteorder;

use self::byteorder::ByteOrder;
use super::bus::{unmapped_area_r, unmapped_area_w, HwIoR, HwIoW};
use super::memint::MemInt;
use std::borrow::{Borrow, BorrowMut};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

bitflags! {
   pub struct MemFlags: u8 {
        const READACCESS = 0b00000001;
        const WRITEACCESS = 0b00000010;
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

    pub fn hwio_r<S: MemInt>(&self) -> HwIoR {
        if !self.flags.contains(MemFlags::READACCESS) {
            return unmapped_area_r();
        }

        HwIoR::Mem(self.buf.clone(), (self.psize - 1) as u32)
    }

    pub fn hwio_w<S: MemInt>(&self) -> HwIoW {
        if !self.flags.contains(MemFlags::WRITEACCESS) {
            return unmapped_area_w();
        }

        HwIoW::Mem(self.buf.clone(), (self.psize - 1) as u32)
    }
}

// impl<O: ByteOrder> Borrow<[u8]> for Mem<O> {
//     fn borrow(&self) -> &[u8] {
//         self.buf.borrow()
//     }
// }

// impl<O: ByteOrder> BorrowMut<[u8]> for Mem<O> {
//     fn borrow_mut(&mut self) -> &mut [u8] {
//         self.buf.borrow_mut()
//     }
// }
