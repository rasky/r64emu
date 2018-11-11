use super::{Sp, StatusFlags};
use emu::bus::be::DevPtr;
use mips64;

pub struct SpCop0 {
    sp: DevPtr<Sp>,
}

impl SpCop0 {
    pub fn new(sp: &DevPtr<Sp>) -> Box<SpCop0> {
        Box::new(SpCop0 { sp: sp.clone() })
    }
}

impl mips64::Cop0 for SpCop0 {
    fn pending_int(&self) -> bool {
        false // RSP generate has no interrupts
    }

    fn exception(&mut self, ctx: &mut mips64::CpuContext, exc: mips64::Exception) {
        match exc {
            mips64::Exception::RESET => {
                ctx.set_pc(0);
            }

            // Breakpoint exception is used by RSP to halt itself
            mips64::Exception::BP => {
                let mut sp = self.sp.borrow_mut();
                let mut status = sp.get_status();
                status.insert(StatusFlags::HALT | StatusFlags::BROKE);
                sp.set_status(status, ctx);
            }
            _ => unimplemented!(),
        }
    }
}

impl mips64::Cop for SpCop0 {
    fn set_reg(&mut self, _idx: usize, _val: u128) {
        panic!("unsupported COP0 reg access in RSP")
    }
    fn reg(&self, _idx: usize) -> u128 {
        panic!("unsupported COP0 reg access in RSP")
    }

    fn op(&mut self, _cpu: &mut mips64::CpuContext, _opcode: u32) {
        panic!("unsupported COP0 opcode in RSP")
    }
}
