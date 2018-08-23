extern crate num;

use self::num::Float;
use super::cpu::{Cop, CpuContext};
use slog;
use std::marker::PhantomData;

pub struct Fpu {
    regs: [u64; 32],
    fir: u64,
    fccr: u64,
    fexr: u64,
    fenr: u64,
    fcsr: u64,

    logger: slog::Logger,
}

trait FloatRawConvert {
    fn from_u64bits(v: u64) -> Self;
    fn to_u64bits(self) -> u64;
    fn bankers_round(self) -> Self;
}

impl FloatRawConvert for f32 {
    fn from_u64bits(v: u64) -> Self {
        f32::from_bits(v as u32)
    }
    fn to_u64bits(self) -> u64 {
        self.to_bits() as u64
    }
    fn bankers_round(self) -> Self {
        let y = self.round();
        if (self - y).abs() == 0.5 {
            (self * 0.5).round() * 2.0
        } else {
            y
        }
    }
}

impl FloatRawConvert for f64 {
    fn from_u64bits(v: u64) -> Self {
        f64::from_bits(v)
    }
    fn to_u64bits(self) -> u64 {
        self.to_bits()
    }
    fn bankers_round(self) -> Self {
        let y = self.round();
        if (self - y).abs() == 0.5 {
            (self * 0.5).round() * 2.0
        } else {
            y
        }
    }
}

struct Fop<'a, F: Float + FloatRawConvert> {
    opcode: u32,
    fpu: &'a mut Fpu,
    cpu: &'a mut CpuContext,
    phantom: PhantomData<F>,
}

impl<'a, F: Float + FloatRawConvert> Fop<'a, F> {
    fn func(&self) -> u32 {
        self.opcode & 0x3f
    }
    fn cc(&self) -> usize {
        ((self.opcode >> 8) & 7) as usize
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
        F::from_u64bits(self.fpu.regs[self.rs()])
    }
    fn ft(&self) -> F {
        F::from_u64bits(self.fpu.regs[self.rt()])
    }
    fn set_fd(&mut self, v: F) {
        self.fpu.regs[self.rd()] = v.to_u64bits();
    }
    fn mfd64(&'a mut self) -> &'a mut u64 {
        &mut self.fpu.regs[self.rd()]
    }
}

macro_rules! approx {
    ($op:ident, $round:ident, $size:ident) => {{
        match $op.fs().$round().$size() {
            Some(v) => *$op.mfd64() = v as u64,
            None => panic!("approx out of range"),
        }
    }};
}

macro_rules! cond {
    ($op:ident, $cond:expr) => {{
        let cond = $cond;
        let cc = $op.cc();
        $op.fpu.set_cc(cc, cond);
    }};
}

impl Fpu {
    pub fn new(logger: slog::Logger) -> Box<Fpu> {
        Box::new(Fpu {
            regs: [0u64; 32],
            fir: 0,
            fccr: 0,
            fexr: 0,
            fenr: 0,
            fcsr: 0,
            logger,
        })
    }

    fn set_cc(&mut self, cc: usize, val: bool) {
        if cc > 8 {
            panic!("invalid cc code");
        }
        self.fccr = (self.fccr & !(1 << cc)) | ((val as u64) << cc);
        let mut cc2 = cc + 23;
        if cc > 0 {
            cc2 += 1;
        }
        self.fcsr = (self.fcsr & !(1 << cc2)) | ((val as u64) << cc2);
    }

    fn get_cc(&mut self, cc: usize) -> bool {
        if cc > 8 {
            panic!("invalid cc code");
        }
        (self.fccr & (1 << cc)) != 0
    }

    fn fop<M: Float + FloatRawConvert>(&mut self, cpu: &mut CpuContext, opcode: u32) {
        let mut op = Fop::<M> {
            opcode,
            fpu: self,
            cpu,
            phantom: PhantomData,
        };
        match op.func() {
            0x00 => {
                // ADD.fmt
                let v = op.fs() + op.ft();
                op.set_fd(v)
            }
            0x01 => {
                // SUB.fmt
                let v = op.fs() - op.ft();
                op.set_fd(v)
            }
            0x02 => {
                // MUL.fmt
                let v = op.fs() * op.ft();
                op.set_fd(v)
            }
            0x03 => {
                // DIV.fmt
                let v = op.fs() / op.ft();
                op.set_fd(v)
            }
            0x04 => {
                // SQRT.fmt
                let v = op.fs().sqrt();
                op.set_fd(v)
            }
            0x05 => {
                // ABS.fmt
                let v = op.fs().abs();
                op.set_fd(v)
            }
            0x07 => {
                // NEG.fmt
                let v = op.fs().neg();
                op.set_fd(v)
            }
            0x08 => approx!(op, bankers_round, to_i64), // ROUND.L.fmt
            0x09 => approx!(op, trunc, to_i64),         // TRUNC.L.fmt
            0x0A => approx!(op, ceil, to_i64),          // CEIL.L.fmt
            0x0B => approx!(op, floor, to_i64),         // FLOOR.L.fmt
            0x0C => approx!(op, bankers_round, to_i32), // ROUND.W.fmt
            0x0D => approx!(op, trunc, to_i32),         // TRUNC.W.fmt
            0x0E => approx!(op, ceil, to_i32),          // CEIL.W.fmt
            0x0F => approx!(op, floor, to_i32),         // FLOOR.W.fmt

            0x30 => cond!(op, false),              // C.F.fmt
            0x32 => cond!(op, op.fs() == op.ft()), // C.EQ.fmt
            0x34 => cond!(op, op.fs() < op.ft()),  // C.OLT.fmt
            0x36 => cond!(op, op.fs() <= op.ft()), // C.OLE.fmt

            _ => panic!("unimplemented COP1 opcode: func={:x?}", op.func()),
        }
    }
}

impl Cop for Fpu {
    fn reg(&self, idx: usize) -> u128 {
        self.regs[idx] as u128
    }
    fn set_reg(&mut self, idx: usize, val: u128) {
        self.regs[idx] = val as u64;
    }

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32) {
        let fmt = (opcode >> 21) & 0x1F;
        match fmt {
            8 => {
                // TODO: what fmt version is this?
                let tgt = cpu.pc + ((opcode & 0xffff) as i16 as i32 as u32) * 4;
                let cc = ((opcode >> 18) & 3) as usize;
                let nd = opcode & (1 << 17) != 0;
                let tf = opcode & (1 << 16) != 0;
                let cond = self.get_cc(cc) == tf;
                cpu.branch(cond, tgt, nd);
            }
            16 => self.fop::<f32>(cpu, opcode),
            17 => self.fop::<f64>(cpu, opcode),
            20 => panic!("unimplemented COP1 fmt: 20 => W"),
            21 => panic!("unimplemented COP1 fmt: 21 => L"),
            18 | 19 | 22...31 => warn!(self.logger, "reserved COP1 fmt:"; "fmt" => fmt),
            _ => warn!(self.logger, "unknown COP1 fmt:"; "fmt" => fmt),
        }
    }
}
