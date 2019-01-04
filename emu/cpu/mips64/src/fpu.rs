use super::decode::{DecodedInsn, MEMOP_FMT, REG_NAMES};
use super::{Cop, CpuContext};

use emu::dbg::{DebuggerRenderer, Operand, RegisterSize, RegisterView, Result, Tracer};
use emu::int::Numerics;
use emu::state::Field;

use num::Float;
use serde_derive::{Deserialize, Serialize};
use slog;
use slog::*;
use std::marker::PhantomData;

const FPU_REG_NAMES: [&'static str; 32] = [
    "f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14",
    "f15", "f16", "f17", "f18", "f19", "f20", "f21", "f22", "f23", "f24", "f25", "f26", "f27",
    "f28", "f29", "f30", "f31",
];

const FPU_CREG_NAMES: [&'static str; 32] = [
    "?0?", "?1?", "?2?", "?3?", "?4?", "?5?", "?6?", "?7?", "?8?", "?9?", "?10?", "?11?", "?12?",
    "?13?", "?14?", "?15?", "?16?", "?17?", "?18?", "?19?", "?20?", "?21?", "?22?", "?23?", "?24?",
    "?25?", "?26?", "?27?", "?28?", "?29?", "?30?", "FCSR",
];

#[derive(Default, Copy, Clone, Serialize, Deserialize)]
struct FpuContext {
    regs: [u64; 32],
    _fir: u64,
    fccr: u64,
    _fexr: u64,
    _fenr: u64,
    fcsr: u64,
    fpu64: bool,
}

impl FpuContext {
    fn get_fgr(&self, idx: usize) -> u64 {
        if self.fpu64 {
            self.regs[idx]
        } else {
            (self.regs[idx + 0] & 0xFFFF_FFFF) | (self.regs[idx + 1] << 32)
        }
    }
    fn set_fgr(&mut self, idx: usize, val: u64) {
        if self.fpu64 {
            self.regs[idx] = val;
        } else {
            self.regs[idx + 0] = val & 0xFFFF_FFFF;
            self.regs[idx + 1] = val >> 32;
        }
    }
    fn get_fpr<F: FloatRawConvert>(&self, idx: usize) -> F {
        F::from_u64bits(self.get_fgr(idx))
    }
    fn set_fpr<F: FloatRawConvert>(&mut self, idx: usize, val: F) {
        self.set_fgr(idx, val.to_u64bits());
    }
}

pub struct Fpu {
    ctx: Field<FpuContext>,
    logger: slog::Logger,
    name: &'static str,
}

trait FloatRawConvert {
    fn from_u64bits(v: u64) -> Self;
    fn to_u64bits(self) -> u64;
    fn bankers_round(self) -> Self;
    fn to_f32(self) -> f32;
    fn to_f64(self) -> f64;
    fn to_u64(self) -> u64;
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
    fn to_f32(self) -> f32 {
        self as f32
    }
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn to_u64(self) -> u64 {
        self as u64
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
    fn to_f32(self) -> f32 {
        self as f32
    }
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn to_u64(self) -> u64 {
        self as u64
    }
}

struct Fop<'a, F: Float + FloatRawConvert> {
    opcode: u32,
    fpu: &'a mut Fpu,
    ctx: &'a mut FpuContext,
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
        self.ctx.get_fpr(self.rs())
    }
    fn fgs(&self) -> u64 {
        self.ctx.get_fgr(self.rs())
    }
    fn ft(&self) -> F {
        self.ctx.get_fpr(self.rt())
    }
    fn fgt(&self) -> u64 {
        self.ctx.get_fgr(self.rt())
    }
    fn set_fd(&mut self, v: F) {
        self.ctx.set_fpr(self.rd(), v);
    }
    fn set_fgd(&mut self, v: u64) {
        self.ctx.set_fgr(self.rd(), v);
    }
}

macro_rules! approx {
    ($op:ident, $t:expr, $round:ident, $size:ident, $fallback:expr) => {{
        match $op.fs().$round().$size() {
            Some(v) => $op.set_fgd(v as u64),
            None => $op.set_fgd($fallback as u64),
        }
    }};
}

macro_rules! cond {
    ($op:ident, $func:expr) => {{
        let fs = $op.fs();
        let ft = $op.ft();
        let nan = fs.is_nan() || ft.is_nan();
        let less = if !nan { fs < ft } else { false };
        let equal = if !nan { fs == ft } else { false };
        if nan && $func & 8 != 0 {
            panic!("signal FPU NaN in comparison");
        }

        let cond =
            (less && ($func & 4) != 0) || (equal && ($func & 2) != 0) || (nan && ($func & 1) != 0);
        let cc = $op.cc();
        $op.fpu.set_cc(cc, cond);
    }};
}

macro_rules! fp_suffix {
    ($name:expr, $op:ident) => {
        match $op {
            0x10 => concat!($name, ".s"),
            0x11 => concat!($name, ".d"),
            0x14 => concat!($name, ".w"),
            0x15 => concat!($name, ".l"),
            _ => unreachable!(),
        }
    };
}

impl Fpu {
    pub fn new(name: &'static str, logger: slog::Logger) -> Fpu {
        Fpu {
            ctx: Field::new(&("mips64::fpu::".to_owned() + name), FpuContext::default()),
            logger,
            name,
        }
    }

    fn set_cc(&mut self, cc: usize, val: bool) {
        if cc > 8 {
            panic!("invalid cc code");
        }
        self.ctx.fccr = (self.ctx.fccr & !(1 << cc)) | ((val as u64) << cc);
        let mut cc2 = cc + 23;
        if cc > 0 {
            cc2 += 1;
        }
        self.ctx.fcsr = (self.ctx.fcsr & !(1 << cc2)) | ((val as u64) << cc2);
    }

    fn get_cc(&mut self, cc: usize) -> bool {
        if cc > 8 {
            panic!("invalid cc code");
        }
        (self.ctx.fccr & (1 << cc)) != 0
    }

    fn fop<M: Float + FloatRawConvert>(
        &mut self,
        cpu: &mut CpuContext,
        opcode: u32,
        t: &Tracer,
    ) -> Result<()> {
        let mut op = Fop::<M> {
            opcode,
            ctx: unsafe { self.ctx.as_mut() },
            fpu: self,
            cpu: cpu,
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
                // MUL.with_fmtt
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
            0x06 => {
                // MOV.fmt
                let v = op.fs();
                op.set_fd(v);
            }
            0x07 => {
                // NEG.fmt
                let v = op.fs().neg();
                op.set_fd(v)
            }
            0x08 => approx!(op, t, bankers_round, to_i64, i64::max_value()), // ROUND.L.fmt
            0x09 => approx!(op, t, trunc, to_i64, i64::max_value()),         // TRUNC.L.fmt
            0x0A => approx!(op, t, ceil, to_i64, i64::max_value()),          // CEIL.L.fmt
            0x0B => approx!(op, t, floor, to_i64, i64::max_value()),         // FLOOR.L.fmt
            0x0C => approx!(op, t, bankers_round, to_i32, i32::max_value()), // ROUND.W.fmt
            0x0D => approx!(op, t, trunc, to_i32, i32::max_value()),         // TRUNC.W.fmt
            0x0E => approx!(op, t, ceil, to_i32, i32::max_value()),          // CEIL.W.fmt
            0x0F => approx!(op, t, floor, to_i32, i32::max_value()),         // FLOOR.W.fmt

            0x20 => op.set_fgd(op.fs().to_f32().to_u64bits()), // CVT.S.fmt
            0x21 => op.set_fgd(op.fs().to_f64().to_u64bits()), // CVT.D.fmt
            0x24 => op.set_fgd(op.fs().to_u64() as u32 as u64), // CVT.W.fmt
            0x25 => op.set_fgd(op.fs().to_u64()),              // CVT.L.fmt

            0x30 => cond!(op, 0x30), // C.T.fmt
            0x31 => cond!(op, 0x31), // C.UN.fmt
            0x32 => cond!(op, 0x32), // C.EQ.fmt
            0x33 => cond!(op, 0x33), // C.UEQ.fmt
            0x34 => cond!(op, 0x34), // C.OLT.fmt
            0x35 => cond!(op, 0x35), // C.ULT.fmt
            0x36 => cond!(op, 0x36), // C.OLE.fmt
            0x37 => cond!(op, 0x37), // C.ULT.fmt
            0x38 => cond!(op, 0x38), // C.SF.fmt
            0x39 => cond!(op, 0x39), // C.NGLE.fmt
            0x3A => cond!(op, 0x3A), // C.SEQ.fmt
            0x3B => cond!(op, 0x3B), // C.NGL.fmt
            0x3C => cond!(op, 0x3C), // C.LT.fmt
            0x3D => cond!(op, 0x3D), // C.NGE.fmt
            0x3E => cond!(op, 0x3E), // C.LE.fmt
            0x3F => cond!(op, 0x3F), // C.NGT.fmt

            _ => {
                error!(
                    op.fpu.logger,
                    "unimplemented COP1 opcode: func={:x?}",
                    op.func()
                );
                return t.break_here("unimplemented COP1 opcode");
            }
        }
        Ok(())
    }
}

impl Cop for Fpu {
    fn reg(&self, _cpu: &CpuContext, idx: usize) -> u128 {
        self.ctx.regs[idx] as u128
    }
    fn set_reg(&mut self, _cpu: &mut CpuContext, idx: usize, val: u128) {
        self.ctx.regs[idx] = val as u64;
    }

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32, t: &Tracer) -> Result<()> {
        self.ctx.fpu64 = cpu.fpu64; // copy current fpu64 mode bit (from COP0)
        let func = opcode & 0x3f;
        let fmt = (opcode >> 21) & 0x1F;
        let rt = ((opcode >> 16) & 0x1F) as usize;
        let rs = ((opcode >> 11) & 0x1F) as usize;
        let rd = ((opcode >> 6) & 0x1F) as usize;
        match fmt {
            0x0 => cpu.regs[rt] = (self.ctx.regs[rs] as u32).sx64(), // MFC1
            0x2 => match rs {
                // CFC1
                31 => cpu.regs[rt] = self.ctx.fcsr,
                _ => {
                    error!(self.logger, "CFC1 from unknown register: {:x}", rs);
                    return t.break_here("CFC1 from unknown register");
                }
            },
            0x4 => self.ctx.regs[rs] = (cpu.regs[rt] as u32) as u64, // MTC1
            0x6 => match rs {
                // CTC1
                31 => self.ctx.fcsr = cpu.regs[rt],
                _ => {
                    error!(self.logger, "CTC1 to unknown register: {:x}", rs);
                    return t.break_here("CTC1 to unknown register");
                }
            },
            0x8 => {
                let tgt = cpu.pc + (opcode as u16).sx64() * 4;
                let cc = ((opcode >> 18) & 3) as usize;
                let nd = opcode & (1 << 17) != 0;
                let tf = opcode & (1 << 16) != 0;
                let cond = self.get_cc(cc) == tf;
                cpu.branch(cond, tgt, nd);
            }
            0x10 => return self.fop::<f32>(cpu, opcode, t),
            0x11 => return self.fop::<f64>(cpu, opcode, t),

            0x14 => match func {
                0x20 => {
                    // CVT.S.W
                    let fgs = self.ctx.get_fgr(rs);
                    self.ctx.set_fpr(rd, fgs as i32 as f32);
                }
                0x21 => {
                    // CVT.D.W
                    let fgs = self.ctx.get_fgr(rs);
                    self.ctx.set_fpr(rd, fgs as i32 as f64);
                }
                _ => {
                    error!(self.logger, "unimplemented COP1 W: func={:x?}", func);
                    return t.break_here("unimplemented COP1 W opcode");
                }
            },
            0x15 => match func {
                0x20 => {
                    // CVT.S.L
                    let fgs = self.ctx.get_fgr(rs);
                    self.ctx.set_fpr(rd, fgs as i64 as f32);
                }
                0x21 => {
                    // CVT.D.L
                    let fgs = self.ctx.get_fgr(rs);
                    self.ctx.set_fpr(rd, fgs as i64 as f64);
                }
                _ => {
                    error!(self.logger, "unimplemented COP1 L: func={:x?}", func);
                    return t.break_here("unimplemented COP1 L opcode");
                }
            },

            _ => {
                error!(self.logger, "unimplemented COP1 fmt: fmt={:x?}", fmt);
                return t.break_here("unimplemented COP1 opcode");
            }
        }
        Ok(())
    }

    fn decode(&self, opcode: u32, pc: u64) -> DecodedInsn {
        use self::Operand::*;
        let op = opcode >> 26;
        let func = opcode & 0x3f;
        match op {
            // Regular COP opcode. FPU is usually installed as COP1
            // (so we'll see 0x11), but let's be generic.
            0x10 | 0x11 | 0x12 | 0x13 => {
                let fmt = (opcode >> 21) & 0x1F;
                let rt = REG_NAMES[((opcode >> 16) & 0x1f) as usize].into();
                let fs = FPU_REG_NAMES[((opcode >> 11) & 0x1f) as usize].into();
                let cfs = FPU_CREG_NAMES[((opcode >> 11) & 0x1f) as usize].into();
                match fmt {
                    0x0 => DecodedInsn::new2("mfc1", OReg(rt), IReg(fs)),
                    0x2 => DecodedInsn::new2("cfc1", OReg(rt), IReg(cfs)),
                    0x4 => DecodedInsn::new2("mtc1", IReg(rt), OReg(fs)),
                    0x6 => DecodedInsn::new2("ctc1", IReg(rt), OReg(cfs)),
                    0x8 => {
                        let tgt = pc + 4 + (opcode as u16).sx64() * 4;
                        let cc = ((opcode >> 18) & 3) as usize;
                        let nd = opcode & (1 << 17) != 0;
                        let tf = opcode & (1 << 16) != 0;
                        let name = if tf {
                            if nd {
                                "bc1tl"
                            } else {
                                "bc1t"
                            }
                        } else {
                            if nd {
                                "bc1fl"
                            } else {
                                "bc1f"
                            }
                        };
                        if cc != 0 {
                            DecodedInsn::new2(name, Imm8(cc as u8), Target(tgt))
                        } else {
                            DecodedInsn::new1(name, Target(tgt))
                        }
                    }
                    // Single/Double precision
                    0x10 | 0x11 => {
                        let ft = FPU_REG_NAMES[((opcode >> 16) & 0x1f) as usize].into();
                        let fd = FPU_REG_NAMES[((opcode >> 6) & 0x1f) as usize].into();
                        match func {
                            0x00 => DecodedInsn::new3(
                                fp_suffix!("add", fmt),
                                OReg(fd),
                                IReg(fs),
                                IReg(ft),
                            ),
                            0x01 => DecodedInsn::new3(
                                fp_suffix!("sub", fmt),
                                OReg(fd),
                                IReg(fs),
                                IReg(ft),
                            ),
                            0x02 => DecodedInsn::new3(
                                fp_suffix!("mul", fmt),
                                OReg(fd),
                                IReg(fs),
                                IReg(ft),
                            ),
                            0x03 => DecodedInsn::new3(
                                fp_suffix!("div", fmt),
                                OReg(fd),
                                IReg(fs),
                                IReg(ft),
                            ),
                            0x06 => DecodedInsn::new2(fp_suffix!("mov", fmt), OReg(fd), IReg(fs)),
                            0x07 => DecodedInsn::new2(fp_suffix!("neg", fmt), OReg(fd), IReg(fs)),
                            0x08 => {
                                DecodedInsn::new2(fp_suffix!("round.l", fmt), OReg(fd), IReg(fs))
                            }
                            0x09 => {
                                DecodedInsn::new2(fp_suffix!("trunc.l", fmt), OReg(fd), IReg(fs))
                            }
                            0x0A => {
                                DecodedInsn::new2(fp_suffix!("ceil.l", fmt), OReg(fd), IReg(fs))
                            }
                            0x0B => {
                                DecodedInsn::new2(fp_suffix!("floor.l", fmt), OReg(fd), IReg(fs))
                            }
                            0x0C => {
                                DecodedInsn::new2(fp_suffix!("round.w", fmt), OReg(fd), IReg(fs))
                            }
                            0x0D => {
                                DecodedInsn::new2(fp_suffix!("trunc.w", fmt), OReg(fd), IReg(fs))
                            }
                            0x0E => {
                                DecodedInsn::new2(fp_suffix!("ceil.w", fmt), OReg(fd), IReg(fs))
                            }
                            0x0F => {
                                DecodedInsn::new2(fp_suffix!("floor.w", fmt), OReg(fd), IReg(fs))
                            }

                            0x20 => DecodedInsn::new2(fp_suffix!("cvt.s", fmt), OReg(fd), IReg(fs)),
                            0x21 => DecodedInsn::new2(fp_suffix!("cvt.d", fmt), OReg(fd), IReg(fs)),
                            0x24 => DecodedInsn::new2(fp_suffix!("cvt.w", fmt), OReg(fd), IReg(fs)),
                            0x25 => DecodedInsn::new2(fp_suffix!("cvt.l", fmt), OReg(fd), IReg(fs)),

                            0x30 => DecodedInsn::new2(fp_suffix!("c.t", fmt), IReg(fs), IReg(ft)),
                            0x31 => DecodedInsn::new2(fp_suffix!("c.un", fmt), IReg(fs), IReg(ft)),
                            0x32 => DecodedInsn::new2(fp_suffix!("c.eq", fmt), IReg(fs), IReg(ft)),
                            0x33 => DecodedInsn::new2(fp_suffix!("c.ueq", fmt), IReg(fs), IReg(ft)),
                            0x34 => DecodedInsn::new2(fp_suffix!("c.olt", fmt), IReg(fs), IReg(ft)),
                            0x35 => DecodedInsn::new2(fp_suffix!("c.ult", fmt), IReg(fs), IReg(ft)),
                            0x36 => DecodedInsn::new2(fp_suffix!("c.ole", fmt), IReg(fs), IReg(ft)),
                            0x37 => DecodedInsn::new2(fp_suffix!("c.ult", fmt), IReg(fs), IReg(ft)),
                            0x38 => DecodedInsn::new2(fp_suffix!("c.sf", fmt), IReg(fs), IReg(ft)),
                            0x39 => {
                                DecodedInsn::new2(fp_suffix!("c.ngle", fmt), IReg(fs), IReg(ft))
                            }
                            0x3A => DecodedInsn::new2(fp_suffix!("c.seq", fmt), IReg(fs), IReg(ft)),
                            0x3B => DecodedInsn::new2(fp_suffix!("c.ngl", fmt), IReg(fs), IReg(ft)),
                            0x3C => DecodedInsn::new2(fp_suffix!("c.lt", fmt), IReg(fs), IReg(ft)),
                            0x3D => DecodedInsn::new2(fp_suffix!("c.nge", fmt), IReg(fs), IReg(ft)),
                            0x3E => DecodedInsn::new2(fp_suffix!("c.le", fmt), IReg(fs), IReg(ft)),
                            0x3F => DecodedInsn::new2(fp_suffix!("c.ngt", fmt), IReg(fs), IReg(ft)),
                            _ => DecodedInsn::new1("cop1op?", Imm32(func)),
                        }
                    }
                    0x14 | 0x15 => {
                        let fd = FPU_REG_NAMES[((opcode >> 6) & 0x1f) as usize].into();
                        match func {
                            0x20 => DecodedInsn::new2(fp_suffix!("cvt.s", fmt), OReg(fd), IReg(fs)),
                            0x21 => DecodedInsn::new2(fp_suffix!("cvt.d", fmt), OReg(fd), IReg(fs)),
                            _ => DecodedInsn::new1("cop1cvt?", Imm32(func)),
                        }
                    }
                    _ => DecodedInsn::new1("cop1?", Imm32(fmt)),
                }
            }
            // LWC1/LWC2/LDC1/LDC2
            0x31 | 0x32 | 0x35 | 0x36 | 0x39 | 0x3A | 0x3D | 0x3E => {
                let name: &str = match op {
                    0x31 => "lwc1",
                    0x32 => "lwc2",
                    0x35 => "ldc1",
                    0x36 => "ldc2",
                    0x39 => "swc1",
                    0x3A => "swc2",
                    0x3D => "sdc1",
                    0x3E => "sdc2",
                    _ => unreachable!(),
                };
                let ft = FPU_REG_NAMES[((opcode >> 16) & 0x1f) as usize].into();
                let rd = REG_NAMES[((opcode >> 21) & 0x1f) as usize].into();
                let off = (opcode & 0xffff) as i16 as i32 as u32;
                if op >= 0x39 {
                    DecodedInsn::new3(name, IReg(ft), Imm32(off), IReg(rd)).with_fmt(MEMOP_FMT)
                } else {
                    DecodedInsn::new3(name, OReg(ft), Imm32(off), IReg(rd)).with_fmt(MEMOP_FMT)
                }
            }
            _ => DecodedInsn::new1("unkfpu", Imm8(op as u8)),
        }
    }

    fn render_debug(&mut self, dr: &DebuggerRenderer) {
        dr.render_regview(self);
    }
}

impl RegisterView for Fpu {
    const WINDOW_SIZE: (f32, f32) = (360.0, 400.0);
    const COLUMNS: usize = 2;

    fn name(&self) -> &str {
        self.name
    }

    fn visit_regs<'s, F>(&'s mut self, col: usize, mut visit: F)
    where
        F: for<'a> FnMut(&'a str, RegisterSize<'a>, Option<&str>),
    {
        use self::RegisterSize::*;

        if self.ctx.fpu64 {
            for idx in 0..16 {
                let idx = idx + col * 8;

                let val = self.ctx.regs[idx];
                let desc = if val >> 32 == 0 {
                    format!("S:{:.5}", f32::from_u64bits(val))
                } else {
                    format!("D:{:.5}", f64::from_u64bits(val))
                };
                visit(
                    FPU_REG_NAMES[idx],
                    Reg64(&mut self.ctx.regs[idx]),
                    Some(&desc),
                );
            }
        } else {
            for idx in 0..8 {
                let idx = idx * 2 + col * 8;

                let val = self.ctx.get_fgr(idx);
                let desc = if val >> 32 == 0 {
                    format!("S:{:.5}", f32::from_u64bits(val))
                } else {
                    format!("D:{:.5}", f64::from_u64bits(val))
                };

                let mut reg0 = self.ctx.regs[idx + 0] as u32;
                let mut reg1 = self.ctx.regs[idx + 1] as u32;
                visit(FPU_REG_NAMES[idx + 0], Reg32(&mut reg0), None);
                visit(FPU_REG_NAMES[idx + 1], Reg32(&mut reg1), Some(&desc));
            }
        }
    }
}
