extern crate num;

use self::num::Float;
use super::cpu::Cpu;
use super::Mipsop;
use std::marker::PhantomData;

#[derive(Default)]
pub(crate) struct Cop1 {
    pub(crate) regs: [u64; 32],
    fir: u64,
    fccr: u64,
    fexr: u64,
    fenr: u64,
    fcsr: u64,
}

trait FloatRawConvert {
    fn from_u64bits(v: u64) -> Self;
    fn to_u64bits(&self) -> u64;
}

impl FloatRawConvert for f32 {
    fn from_u64bits(v: u64) -> Self {
        f32::from_bits(v as u32)
    }
    fn to_u64bits(&self) -> u64 {
        self.to_bits() as u64
    }
}

impl FloatRawConvert for f64 {
    fn from_u64bits(v: u64) -> Self {
        f64::from_bits(v)
    }
    fn to_u64bits(&self) -> u64 {
        self.to_bits()
    }
}

struct Fop<'a, F: Float + FloatRawConvert> {
    opcode: u32,
    cpu: &'a mut Cpu,
    phantom: PhantomData<F>,
}

impl<'a, F: Float + FloatRawConvert> Fop<'a, F> {
    fn func(&self) -> u32 {
        self.opcode & 0x3f
    }
    fn rs(&self) -> usize {
        ((self.opcode >> 11) & 0x1f) as usize
    }
    fn rt(&self) -> usize {
        ((self.opcode >> 16) & 0x1f) as usize
    }
    fn rd(&self) -> usize {
        ((self.opcode >> 6) & 0x1f) as usize
    }
    fn fs(&self) -> F {
        F::from_u64bits(self.cpu.cop1.regs[self.rs()])
    }
    fn ft(&self) -> F {
        F::from_u64bits(self.cpu.cop1.regs[self.rt()])
    }
    fn set_fd(&mut self, v: F) {
        self.cpu.cop1.regs[self.rd()] = v.to_u64bits();
    }
}

impl Cop1 {
    fn fop<M: Float + FloatRawConvert>(cpu: &mut Cpu, opcode: u32) {
        let mut op = Fop::<M> {
            opcode,
            cpu,
            phantom: PhantomData,
        };
        match op.func() {
            0x00 => {
                // ADD.fmt
                let v = op.fs() + op.ft();
                op.set_fd(v)
            }
            _ => panic!("unimplemented COP1 opcode: func={:x?}", op.func()),
        }
    }

    #[inline(always)]
    pub(crate) fn op(cpu: &mut Cpu, opcode: u32) {
        let fmt = (opcode >> 21) & 0x1F;
        match fmt {
            16 => Cop1::fop::<f32>(cpu, opcode),
            17 => Cop1::fop::<f64>(cpu, opcode),
            _ => panic!("unimplemented COP1 fmt: fmt={:x?}", fmt),
        }
    }
}
