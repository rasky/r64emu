extern crate emu;

use super::sp::Sp;
use emu::bus::be::{Bus, DevPtr, Mem, Reg32};
use mips64::{Cop, CpuContext};
use std::cell::RefCell;
use std::rc::Rc;

pub struct SpVector {
    vregs: [u128; 32],
    sp: DevPtr<Sp>,
}

impl SpVector {
    pub fn new(sp: &DevPtr<Sp>) -> Box<SpVector> {
        Box::new(SpVector {
            vregs: [0u128; 32],
            sp: sp.clone(),
        })
    }
}

impl Cop for SpVector {
    fn reg(&mut self, idx: usize) -> &mut u64 {
        unimplemented!()
    }

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32) {
        unimplemented!()
    }

    fn lwc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        unimplemented!()
    }
    fn ldc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        unimplemented!()
    }
    fn swc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        unimplemented!()
    }
    fn sdc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        unimplemented!()
    }
}
