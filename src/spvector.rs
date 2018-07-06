extern crate emu;

use super::sp::Sp;
use byteorder::{BigEndian, ByteOrder};
use emu::bus::be::{Bus, DevPtr};
use emu::int::Numerics;
use mips64::{Cop, CpuContext};
use slog;
use std::arch::x86_64::*;
use std::cell::RefCell;
use std::rc::Rc;

// Vector registers as array of u8.
// We define a separate structure for this array to be able
// to specify alignment, since these will be used with SSE intrinsics.
#[repr(align(16))]
struct VectorRegs([[u8; 16]; 32]);

#[repr(align(16))]
struct AccumRegs([[u8; 8]; 8]);

pub struct SpVector {
    vregs: VectorRegs,
    accum: AccumRegs,
    sp: DevPtr<Sp>,
    logger: slog::Logger,
}

impl SpVector {
    pub fn new(sp: &DevPtr<Sp>, logger: slog::Logger) -> Box<SpVector> {
        Box::new(SpVector {
            vregs: VectorRegs([[0u8; 16]; 32]),
            accum: AccumRegs([[0u8; 8]; 8]),
            sp: sp.clone(),
            logger,
        })
    }

    fn oploadstore(op: u32, ctx: &CpuContext) -> (u32, usize, u32, u32, u32) {
        let base = ctx.regs[((op >> 21) & 0x1F) as usize] as u32;
        let vt = ((op >> 16) & 0x1F) as usize;
        let opcode = (op >> 11) & 0x1F;
        let element = (op >> 7) & 0xF;
        let offset = op & 0x7F;
        (base, vt, opcode, element, offset)
    }
}

struct Vectorop<'a> {
    op: u32,
    spv: &'a mut SpVector,
}

impl<'a> Vectorop<'a> {
    fn func(&self) -> u32 {
        self.op & 0x3F
    }
    fn e(&self) -> usize {
        ((self.op >> 21) & 0xF) as usize
    }
    fn rs(&self) -> usize {
        ((self.op >> 11) & 0x1F) as usize
    }
    fn rt(&self) -> usize {
        ((self.op >> 16) & 0x1F) as usize
    }
    fn rd(&self) -> usize {
        ((self.op >> 6) & 0x1F) as usize
    }
    fn vs(&self) -> __m128i {
        unsafe { _mm_loadu_si128(self.spv.vregs.0[self.rs()].as_ptr() as *const _) }
    }
    fn vt(&self) -> __m128i {
        unsafe { _mm_loadu_si128(self.spv.vregs.0[self.rt()].as_ptr() as *const _) }
    }
    fn setvd(&mut self, val: __m128i) {
        unsafe {
            let rt = self.rt();
            _mm_store_si128(self.spv.vregs.0[rt].as_ptr() as *mut _, val);
        }
    }
    fn accum(&mut self, idx: usize) -> __m128i {
        unsafe { _mm_loadu_si128(self.spv.accum.0[idx..].as_ptr() as *const _) }
    }
}

impl Cop for SpVector {
    fn reg(&mut self, _idx: usize) -> &mut u64 {
        unimplemented!()
    }

    fn op(&mut self, _cpu: &mut CpuContext, op: u32) {
        let mut op = Vectorop { op, spv: self };
        unsafe {
            match op.func() {
                0x10 => {
                    // VADD
                    if op.e() != 0 {
                        unimplemented!();
                    }
                    let vs = op.vs();
                    let vt = op.vt();
                    let res = _mm_adds_epi16(vs, vt);
                    op.setvd(res);
                }
                0x1D => {
                    // VSAR
                    let mut a0 = op.accum(0);
                    let mut a1 = op.accum(2);
                    let mut a2 = op.accum(4);
                    let mut a3 = op.accum(6);
                    // FIXME: refactor using _mm_shuffle*
                    let mask = _mm_set_epi64x(0xFFFF, 0xFFFF);
                    let count = _mm_set_epi64x(16, 16);
                    a0 = _mm_srl_epi64(a0, count);
                    a1 = _mm_srl_epi64(a1, count);
                    a2 = _mm_srl_epi64(a2, count);
                    a3 = _mm_srl_epi64(a3, count);
                    a0 = _mm_and_si128(a0, mask);
                    a1 = _mm_and_si128(a1, mask);
                    a2 = _mm_and_si128(a2, mask);
                    a3 = _mm_and_si128(a3, mask);
                    let b0 = _mm_packs_epi32(a0, a1);
                    let b1 = _mm_packs_epi32(a2, a3);
                    let c0 = _mm_packs_epi32(b0, b1);
                    op.setvd(c0);
                }
                _ => panic!("unimplemented VU opcode={}", op.func().hex()),
            }
        }
    }

    fn lwc(&mut self, op: u32, ctx: &CpuContext, _bus: &Rc<RefCell<Box<Bus>>>) {
        let sp = self.sp.borrow();
        let dmem = sp.dmem.buf();
        let (base, vt, op, element, offset) = SpVector::oploadstore(op, ctx);
        match op {
            0x04 => {
                // LQV
                let ea = ((base + (offset << 4)) & 0xFFF) as usize;
                let ea_end = (ea & !0xF) + 0x10;
                for (m, r) in dmem[ea..ea_end]
                    .iter()
                    .zip(self.vregs.0[vt].iter_mut().rev())
                {
                    *r = *m;
                }
            }
            _ => panic!("unimplemented VU load opcode={}", op.hex()),
        }
    }
    fn swc(&mut self, op: u32, ctx: &CpuContext, _bus: &Rc<RefCell<Box<Bus>>>) {
        let sp = self.sp.borrow();
        let mut dmem = sp.dmem.buf();
        let (base, vt, op, element, offset) = SpVector::oploadstore(op, ctx);
        match op {
            0x04 => {
                // SQV
                let ea = ((base + (offset << 4)) & 0xFFF) as usize;
                let ea_end = (ea & !0xF) + 0x10;
                for (m, r) in dmem[ea..ea_end]
                    .iter_mut()
                    .zip(self.vregs.0[vt].iter().rev())
                {
                    *m = *r;
                }
            }
            _ => panic!("unimplemented VU load opcode={}", op.hex()),
        }
    }

    fn ldc(&mut self, _op: u32, _ctx: &CpuContext, _bus: &Rc<RefCell<Box<Bus>>>) {
        unimplemented!()
    }
    fn sdc(&mut self, _op: u32, _ctx: &CpuContext, _bus: &Rc<RefCell<Box<Bus>>>) {
        unimplemented!()
    }
}
