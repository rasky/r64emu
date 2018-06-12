extern crate byteorder;

use self::byteorder::ByteOrder;
use super::bus::{MemIoR, MemIoW, RawPtr, RawPtrMut};
use super::memint::MemInt;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::mem;

trait Register {
    type U: MemInt;
}

trait RegBank {
    fn get_regs<'a, U: MemInt>(&'a self) -> Vec<&(Register<U = U> + 'a)>;
}

pub struct Reg<O,U>
where
    O: ByteOrder,
    U: MemInt,
{
    raw: RefCell<[u8; 8]>,
    romask: U,
    wcb: Option<fn(old: U, new: U)>,
    rcb: Option<fn(cur: U) -> U>,
    phantom: PhantomData<O>,
}

impl<O,U> Reg<O,U>
where
    O: ByteOrder,
    U: MemInt,
{
    fn raw_get(&self) -> U {
        U::endian_read_from::<O>(&self.raw.borrow()[..])
    }

    fn raw_set(&self, val: U) {
        U::endian_write_to::<O>(&mut self.raw.borrow_mut()[..], val)
    }

    fn mem_io_r(&self) -> MemIoR<O, U> {
        match self.rcb {
            Some(f) => MemIoR::Func(Box::new(move || f(self.raw_get()).into())),
            None => MemIoR::Raw(RawPtr(&self.raw.borrow()[0])),
        }
    }

    fn mem_io_w(&mut self) -> MemIoW<O, U> {
        if self.romask == U::zero() && self.wcb.is_none() {
            MemIoW::Raw(RawPtrMut(&mut self.raw.borrow_mut()[0]))
        } else {
            MemIoW::Func(Box::new(move |val64| {
                let mut val = U::truncate_from(val64);
                let old = self.raw_get();
                val = (val & !self.romask) | (old & self.romask);
                self.raw_set(val);
                self.wcb.map(|f| f(old, val));
            }))
        }
    }

    pub fn get(&self) -> U {
        self.mem_io_r().read()
    }

    pub fn set(&mut self, val: U) {
        self.mem_io_w().write(val);
    }
}
