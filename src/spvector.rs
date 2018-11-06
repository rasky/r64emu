extern crate emu;

// Select SSE version used by RSP vector code
// FIXME: eventually, this can be a runtime switch
use super::vops::sse41 as vops;

use super::sp::Sp;
use byteorder::{ByteOrder, LittleEndian};
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

#[derive(Debug, Copy, Clone)]
#[repr(align(16))]
struct VectorReg([u8; 16]);

pub struct SpVector {
    vregs: VectorRegs,
    accum: [VectorReg; 3],
    vco_carry: VectorReg,
    vco_ne: VectorReg,
    sp: DevPtr<Sp>,
    logger: slog::Logger,
}

impl SpVector {
    pub const REG_VCO: usize = 32;
    pub const REG_VCC: usize = 33;
    pub const REG_VCE: usize = 34;
    pub const REG_ACCUM_LO: usize = 35;
    pub const REG_ACCUM_MD: usize = 36;
    pub const REG_ACCUM_HI: usize = 37;

    pub fn new(sp: &DevPtr<Sp>, logger: slog::Logger) -> Box<SpVector> {
        Box::new(SpVector {
            vregs: VectorRegs([[0u8; 16]; 32]),
            accum: [VectorReg([0u8; 16]); 3],
            vco_carry: VectorReg([0u8; 16]),
            vco_ne: VectorReg([0u8; 16]),
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

    fn vce(&self) -> u16 {
        0
    }
    fn set_vce(&self, _vec: u16) {}

    fn vcc(&self) -> u16 {
        0
    }
    fn set_vcc(&self, _vec: u16) {}

    fn vco(&self) -> u16 {
        let mut res = 0u16;
        for i in 0..8 {
            res |= LittleEndian::read_u16(&self.vco_carry.0[(7 - i) * 2..]) << i;
            res |= LittleEndian::read_u16(&self.vco_ne.0[(7 - i) * 2..]) << (i + 8);
        }
        res
    }

    fn set_vco(&mut self, vco: u16) {
        for i in 0..8 {
            LittleEndian::write_u16(&mut self.vco_carry.0[(7 - i) * 2..], (vco >> i) & 1);
            LittleEndian::write_u16(&mut self.vco_ne.0[(7 - i) * 2..], (vco >> (i + 8)) & 1);
        }
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
    fn vte(&self) -> __m128i {
        let e = self.e();
        if e != 0 {
            unimplemented!();
        }
        unsafe { _mm_loadu_si128(self.spv.vregs.0[self.rt()].as_ptr() as *const _) }
    }
    fn setvd(&mut self, val: __m128i) {
        unsafe {
            let rd = self.rd();
            _mm_store_si128(self.spv.vregs.0[rd].as_ptr() as *mut _, val);
        }
    }
    fn accum(&self, idx: usize) -> __m128i {
        unsafe { _mm_loadu_si128(self.spv.accum[idx].0.as_ptr() as *const _) }
    }
    fn setaccum(&mut self, idx: usize, val: __m128i) {
        unsafe { _mm_store_si128(self.spv.accum[idx].0.as_ptr() as *mut _, val) }
    }
    fn carry(&self) -> __m128i {
        unsafe { _mm_loadu_si128(self.spv.vco_carry.0.as_ptr() as *const _) }
    }
    fn setcarry(&self, val: __m128i) {
        unsafe { _mm_store_si128(self.spv.vco_carry.0.as_ptr() as *mut _, val) }
    }
    fn setne(&self, val: __m128i) {
        unsafe { _mm_store_si128(self.spv.vco_ne.0.as_ptr() as *mut _, val) }
    }
}

macro_rules! op_vmul {
    ($op:expr, $name:ident) => {{
        let (res, acc_lo, acc_md, acc_hi) = vops::$name(
            $op.vs(),
            $op.vte(),
            $op.accum(0),
            $op.accum(1),
            $op.accum(2),
        );
        $op.setvd(res);
        $op.setaccum(0, acc_lo);
        $op.setaccum(1, acc_md);
        $op.setaccum(2, acc_hi);
    }};
}

impl SpVector {
    #[target_feature(enable = "sse4.1")]
    unsafe fn uop(&mut self, cpu: &mut CpuContext, op: u32) {
        let mut op = Vectorop { op, spv: self };
        let vzero = _mm_setzero_si128();
        if op.op & (1 << 25) != 0 {
            match op.func() {
                0x00 => op_vmul!(op, vmulf), // VMULF
                0x01 => op_vmul!(op, vmulu), // VMULU
                0x04 => op_vmul!(op, vmudl), // VMUDL
                0x06 => op_vmul!(op, vmudn), // VMUDN
                0x07 => op_vmul!(op, vmudh), // VMUDH
                0x08 => op_vmul!(op, vmacf), // VMACF
                0x09 => op_vmul!(op, vmacu), // VMACU
                0x0C => op_vmul!(op, vmadl), // VMADL
                0x0E => op_vmul!(op, vmadn), // VMADN
                0x0F => op_vmul!(op, vmadh), // VMADH
                0x10 => {
                    // VADD
                    let vs = op.vs();
                    let vt = op.vte();
                    let carry = op.carry();

                    // Add the carry to the minimum value, as we need to
                    // saturate the final result and not only intermediate
                    // results:
                    //     0x8000 + 0x8000 + 0x1 must be 0x8000, not 0x8001
                    let min = _mm_min_epi16(vs, vt);
                    let max = _mm_max_epi16(vs, vt);
                    op.setvd(_mm_adds_epi16(_mm_adds_epi16(min, carry), max));
                    op.setaccum(0, _mm_add_epi16(_mm_add_epi16(vs, vt), carry));
                    op.setcarry(vzero);
                    op.setne(vzero);
                }
                0x14 => {
                    // VADDC
                    let vs = op.vs();
                    let vt = op.vte();
                    let res = _mm_add_epi16(vs, vt);
                    op.setvd(res);
                    op.setaccum(0, res);
                    op.setne(vzero);

                    // We need to compute the carry bit. To do so, we use signed
                    // comparison of 16-bit integers, xoring with 0x8000 to obtain
                    // the unsigned result.
                    #[allow(overflowing_literals)]
                    let mask = _mm_set1_epi16(0x8000);
                    let carry = _mm_cmpgt_epi16(_mm_xor_si128(mask, vs), _mm_xor_si128(mask, res));
                    op.setcarry(_mm_srli_epi16(carry, 15));
                }
                0x1D => {
                    // VSAR
                    let e = op.e();
                    match e {
                        0..=2 => {
                            op.setvd(vzero);
                        }
                        8..=10 => {
                            // NOTE: VSAR is not able to write the accumulator,
                            // contrary to what documentation says.
                            let sar = op.accum(2 - (e - 8));
                            op.setvd(sar);
                        }
                        _ => unimplemented!(),
                    }
                }
                0x28 => {
                    // VAND
                    let res = _mm_and_si128(op.vs(), op.vte());
                    op.setvd(res);
                    op.setaccum(0, res);
                }
                0x29 => {
                    // VNAND
                    let res = _mm_xor_si128(_mm_and_si128(op.vs(), op.vte()), _mm_set1_epi16(-1));
                    op.setvd(res);
                    op.setaccum(0, res);
                }
                0x2A => {
                    // VOR
                    let res = _mm_or_si128(op.vs(), op.vte());
                    op.setvd(res);
                    op.setaccum(0, res);
                }
                0x2B => {
                    // VNOR
                    let res = _mm_xor_si128(_mm_or_si128(op.vs(), op.vte()), _mm_set1_epi16(-1));
                    op.setvd(res);
                    op.setaccum(0, res);
                }
                0x2C => {
                    // VXOR
                    let res = _mm_xor_si128(op.vs(), op.vte());
                    op.setvd(res);
                    op.setaccum(0, res);
                }
                0x2D => {
                    // VNXOR
                    let res = _mm_xor_si128(_mm_xor_si128(op.vs(), op.vte()), _mm_set1_epi16(-1));
                    op.setvd(res);
                    op.setaccum(0, res);
                }
                _ => panic!("unimplemented COP2 VU opcode={}", op.func().hex()),
            }
        } else {
            match op.e() {
                0x2 => match op.rs() {
                    0 => cpu.regs[op.rt()] = op.spv.vco() as u64,
                    1 => cpu.regs[op.rt()] = op.spv.vcc() as u64,
                    2 => cpu.regs[op.rt()] = op.spv.vce() as u64,
                    _ => panic!("unimplement COP2 CFC2 reg:{}", op.rs()),
                },
                0x6 => match op.rs() {
                    0 => op.spv.set_vco(cpu.regs[op.rt()] as u16),
                    1 => op.spv.set_vcc(cpu.regs[op.rt()] as u16),
                    2 => op.spv.set_vce(cpu.regs[op.rt()] as u16),
                    _ => panic!("unimplement COP2 CTC2 reg:{}", op.rd()),
                },
                _ => panic!("unimplemented COP2 non-VU opcode={:x}", op.e()),
            }
        }
    }
}

unsafe fn to_u128(x: __m128i) -> u128 {
    let v: [u8; 16] = [0u8; 16];
    _mm_store_si128(v.as_ptr() as *mut _, x);
    LittleEndian::read_u128(&v)
}

impl Cop for SpVector {
    fn reg(&self, idx: usize) -> u128 {
        match idx {
            SpVector::REG_VCO => self.vco() as u128,
            SpVector::REG_VCC => self.vcc() as u128,
            SpVector::REG_VCE => self.vce() as u128,
            SpVector::REG_ACCUM_LO => LittleEndian::read_u128(&self.accum[0].0),
            SpVector::REG_ACCUM_MD => LittleEndian::read_u128(&self.accum[1].0),
            SpVector::REG_ACCUM_HI => LittleEndian::read_u128(&self.accum[2].0),
            _ => LittleEndian::read_u128(&self.vregs.0[idx]),
        }
    }
    fn set_reg(&mut self, idx: usize, val: u128) {
        match idx {
            SpVector::REG_VCO => self.set_vco(val as u16),
            SpVector::REG_VCC => self.set_vcc(val as u16),
            SpVector::REG_VCE => self.set_vce(val as u16),
            SpVector::REG_ACCUM_LO => LittleEndian::write_u128(&mut self.accum[0].0, val),
            SpVector::REG_ACCUM_MD => LittleEndian::write_u128(&mut self.accum[1].0, val),
            SpVector::REG_ACCUM_HI => LittleEndian::write_u128(&mut self.accum[2].0, val),
            _ => LittleEndian::write_u128(&mut self.vregs.0[idx], val),
        }
    }

    fn op(&mut self, cpu: &mut CpuContext, op: u32) {
        unsafe { self.uop(cpu, op) }
    }

    fn lwc(&mut self, op: u32, ctx: &CpuContext, _bus: &Rc<RefCell<Box<Bus>>>) {
        let sp = self.sp.borrow();
        let dmem = sp.dmem.buf();
        let (base, vt, op, _element, offset) = SpVector::oploadstore(op, ctx);
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
        let (base, vt, op, _element, offset) = SpVector::oploadstore(op, ctx);
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
