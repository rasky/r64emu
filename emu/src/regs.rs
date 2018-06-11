extern crate byteorder;

use self::byteorder::{ByteOrder, LittleEndian};
use super::bus::{MemIoR, MemIoW, RawPtr, RawPtrMut};

struct Reg32 {
    raw: [u8; 4],
    romask: u32,
    wcb: Option<fn(old: u32, new: u32)>,
    rcb: Option<fn(cur: u32) -> u32>,
}

impl Reg32 {
    fn get(&self) -> u32 {
        LittleEndian::read_u32(&self.raw)
    }

    fn set(&mut self, val: u32) {
        LittleEndian::write_u32(&mut self.raw, val)
    }

    fn mem_io_r32(&self, _pc: u32) -> MemIoR<LittleEndian,u32> {
        match self.rcb {
            Some(f) => MemIoR::Func(Box::new(move || f(self.get()) as u64)),
            None => MemIoR::Raw(RawPtr(&self.raw[0])),
        }
    }

    fn mem_io_w32(&mut self, _pc: u32) -> MemIoW {
        if self.romask == 0 && self.wcb.is_none() {
            MemIoW::Raw(RawPtrMut(&mut self.raw[0]))
        } else {
            MemIoW::Func(Box::new(move |val64| {
                let mut val = val64 as u32;
                let old = self.get();
                val = (val & !self.romask) | (old & self.romask);
                self.set(val);
                self.wcb.map(|f| f(old, val));
            }))
        }
    }
}
