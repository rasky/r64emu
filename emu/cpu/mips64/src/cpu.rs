use super::decode::decode;
use super::mmu::Mmu;
use super::{Arch, Config, Cop, Cop0};

use emu::bus::be::{Bus, MemIoR};
use emu::dbg::{DebuggerRenderer, DisasmView, RegisterSize, RegisterView, Result, Tracer};
use emu::int::Numerics;
use emu::memint::MemInt;
use emu::state::Field;
use emu::sync;

use byteorder::ByteOrder;
use serde_derive::{Deserialize, Serialize};
use slog;

#[derive(Copy, Clone, Debug)]
pub enum Exception {
    Interrupt,  // Interrupt
    Breakpoint, // Breakpoint
    ColdReset,
    SoftReset,
    Nmi,
    TlbRefill,
    XTlbRefill,
}

impl Exception {
    pub(crate) fn exc_code(&self) -> Option<u32> {
        match self {
            Exception::Interrupt => Some(0x00),
            Exception::Breakpoint => Some(0x09),
            Exception::ColdReset => None,
            Exception::Nmi => None,
            Exception::SoftReset => None,
            Exception::TlbRefill => None,
            Exception::XTlbRefill => None,
        }
    }
}

#[derive(Default, Copy, Clone, Serialize, Deserialize)]
struct Lines {
    halt: bool,
}

#[derive(Default, Copy, Clone, Serialize, Deserialize)]
pub struct CpuContext {
    pub regs: [u64; 32],
    pub hi: u64,
    pub lo: u64,
    pub pc: u64,
    pub next_pc: u64,
    pub clock: i64,
    pub tight_exit: bool,
    pub delay_slot: bool,
    pub mmu: Mmu,
    pub fpu64: bool,      // True if the FPU (if any) is in 64-bit mode
    lines: Lines,
}

pub struct Cpu<C: Config> {
    pub bus: Box<Bus>,
    pub cop0: C::Cop0,
    pub cop1: C::Cop1,
    pub cop2: C::Cop2,
    pub cop3: C::Cop3,

    ctx: Field<CpuContext>,

    name: String,
    logger: slog::Logger,
    until: i64,
    last_busycheck: u64,
}

struct Mipsop<'a, C: Config> {
    ctx: &'a mut CpuContext,
    opcode: u32,
    cpu: &'a mut Cpu<C>,
}

impl<'a, C: Config> Mipsop<'a, C> {
    fn op(&self) -> u32 {
        self.opcode >> 26
    }
    fn special(&self) -> u32 {
        self.opcode & 0x3f
    }
    fn ea(&self) -> u32 {
        self.rs32() + self.sximm32() as u32
    }
    fn sa(&self) -> usize {
        ((self.opcode >> 6) & 0x1f) as usize
    }
    fn btgt(&self) -> u64 {
        self.ctx.pc + self.sximm64() as u64 * 4
    }
    fn jtgt(&self) -> u64 {
        (self.ctx.pc & 0xFFFF_FFFF_F000_0000) + ((self.opcode & 0x03FF_FFFF) * 4) as u64
    }
    fn rs(&self) -> usize {
        ((self.opcode >> 21) & 0x1f) as usize
    }
    fn rt(&self) -> usize {
        ((self.opcode >> 16) & 0x1f) as usize
    }
    fn rd(&self) -> usize {
        ((self.opcode >> 11) & 0x1f) as usize
    }
    fn sximm32(&self) -> i32 {
        (self.opcode & 0xffff) as i16 as i32
    }
    fn sximm64(&self) -> i64 {
        (self.opcode & 0xffff) as i16 as i64
    }
    fn imm64(&self) -> u64 {
        (self.opcode & 0xffff) as u64
    }
    fn rs64(&self) -> u64 {
        self.ctx.regs[self.rs()]
    }
    fn rt64(&self) -> u64 {
        self.ctx.regs[self.rt()]
    }
    fn irs64(&self) -> i64 {
        self.rs64() as i64
    }
    fn irt64(&self) -> i64 {
        self.rt64() as i64
    }
    fn rs32(&self) -> u32 {
        self.rs64() as u32
    }
    fn rt32(&self) -> u32 {
        self.rt64() as u32
    }
    fn irs32(&self) -> i32 {
        self.rs64() as i32
    }
    fn irt32(&self) -> i32 {
        self.rt64() as i32
    }
    fn mrt64(&'a mut self) -> &'a mut u64 {
        let rt = self.rt();
        &mut self.ctx.regs[rt]
    }
    fn mrd64(&'a mut self) -> &'a mut u64 {
        let rd = self.rd();
        &mut self.ctx.regs[rd]
    }
}

impl CpuContext {
    #[inline]
    pub fn branch(&mut self, cond: bool, tgt: u64, likely: bool) {
        if cond {
            self.next_pc = tgt;
            self.delay_slot = true;
        } else if likely {
            // branch not taken; if likely, skip delay slot
            self.pc += 4;
            self.next_pc = self.pc + 4;
            self.clock += 1;
            self.tight_exit = true;
        }
    }

    pub fn set_halt_line(&mut self, stat: bool) {
        self.lines.halt = stat;
        self.tight_exit = true;
    }

    // Directly set PC to a specific value. Used at reset,
    // ERET, and exceptions to do a non-delayed-slot branch.
    pub fn set_pc(&mut self, pc: u64) {
        self.pc = pc;
        self.next_pc = pc + 4;
        self.tight_exit = true;
    }

    pub fn get_pc(&self) -> u64 {
        self.pc
    }
}

macro_rules! branch {
    ($op:ident, $cond:expr, $tgt:expr) => {{
        branch!($op, $cond, $tgt, link(false), likely(false));
    }};
    ($op:ident, $cond:expr, $tgt:expr,link($link:expr)) => {{
        branch!($op, $cond, $tgt, link($link), likely(true));
    }};
    ($op:ident, $cond:expr, $tgt:expr,likely($lkl:expr)) => {{
        branch!($op, $cond, $tgt, link(false), likely($lkl));
    }};
    ($op:ident, $cond:expr, $tgt:expr,link($link:expr),likely($lkl:expr)) => {{
        if $link {
            $op.ctx.regs[31] = $op.ctx.pc + 4;
        }
        let (cond, tgt) = ($cond, $tgt);
        $op.ctx.branch(cond, tgt, $lkl);
        if cond {
            if tgt == $op.ctx.pc - 4 {
                $op.cpu.detect_busywait_2insn($op.ctx.pc - 4);
            } else if tgt == $op.ctx.pc - 8 {
                $op.cpu.detect_busywait_3insn($op.ctx.pc - 4);
            }
        }
    }};
}

macro_rules! check_overflow_add {
    ($op:ident, $dest:expr, $reg1:expr, $reg2:expr) => {{
        match $reg1.checked_add($reg2) {
            Some(res) => $dest = res.sx64(),
            None => $op.cpu.trap_overflow(),
        }
    }};
}

macro_rules! check_overflow_sub {
    ($op:ident, $dest:expr, $reg1:expr, $reg2:expr) => {{
        match $reg1.checked_sub($reg2) {
            Some(res) => $dest = res.sx64(),
            None => $op.cpu.trap_overflow(),
        }
    }};
}

macro_rules! if_cop {
    ($op:ident, $cop:ident, $do:expr) => {{
        if !$op.cpu.$cop.is_null_obj() {
            let $cop = &mut $op.cpu.$cop;
            $do
        } else {
            let pc = $op.ctx.pc;
            let opcode = $op.opcode;
            warn!($op.cpu.logger, "COP opcode without COP";
                "pc" => pc.hex(), "op" => opcode.hex());
        }
    }};
}

impl<C: Config> Cpu<C> {
    pub fn new(
        name: &str,
        logger: slog::Logger,
        bus: Box<Bus>,
        cops: (C::Cop0, C::Cop1, C::Cop2, C::Cop3),
    ) -> Self {
        let mut cpu = Cpu {
            ctx: Field::new(&("mips64::".to_owned() + name), CpuContext::default()),
            bus: bus,
            name: name.into(),
            cop0: cops.0,
            cop1: cops.1,
            cop2: cops.2,
            cop3: cops.3,
            logger: logger,
            until: 0,
            last_busycheck: 0,
        };
        cpu.exception(Exception::ColdReset); // Trigger a reset exception at startup
        cpu
    }

    pub fn ctx(&self) -> &CpuContext {
        &self.ctx
    }

    pub fn ctx_mut(&mut self) -> &mut CpuContext {
        &mut self.ctx
    }

    pub fn reset(&mut self) {
        self.exception(Exception::SoftReset);
    }

    fn exception(&mut self, exc: Exception) {
        self.cop0.exception(&mut self.ctx, exc);
    }

    fn trap_overflow(&mut self) {
        unimplemented!();
    }

    #[inline(always)]
    fn op(&mut self, ctx: &mut CpuContext, opcode: u32, t: &Tracer) -> Result<()> {
        ctx.clock += 1;
        let mut op = Mipsop {
            ctx,
            opcode,
            cpu: self,
        };
        let h = |s| C::Arch::has_op(s);
        match op.op() {
            // SPECIAL
            0x00 => match op.special() {
                0x00 if h("sll") => *op.mrd64() = (op.rt32() << op.sa()).sx64(), // SLL
                0x02 if h("srl") => *op.mrd64() = (op.rt32() >> op.sa()).sx64(), // SRL
                0x03 if h("sra") => *op.mrd64() = (op.irt32() >> op.sa()).sx64(), // SRA
                0x04 if h("sllv") => *op.mrd64() = (op.rt32() << (op.rs32() & 0x1F)).sx64(), // SLLV
                0x06 if h("srll") => *op.mrd64() = (op.rt32() >> (op.rs32() & 0x1F)).sx64(), // SRLV
                0x07 if h("srav") => *op.mrd64() = (op.irt32() >> (op.rs32() & 0x1F)).sx64(), // SRAV
                0x08 if h("jr") => branch!(op, true, op.rs64(), link(false)),                 // JR
                0x09 if h("jalr") => branch!(op, true, op.rs64(), link(true)), // JALR
                0x0D if h("break") => op.cpu.exception(Exception::Breakpoint), // BREAK
                0x0F if h("sync") => {}                                        // SYNC

                0x10 if h("mfhi") => *op.mrd64() = op.ctx.hi, // MFHI
                0x11 if h("mthi") => op.ctx.hi = op.rs64(),   // MTHI
                0x12 if h("mflo") => *op.mrd64() = op.ctx.lo, // MFLO
                0x13 if h("mtlo") => op.ctx.lo = op.rs64(),   // MTLO
                0x14 if h("dsllv") => *op.mrd64() = op.rt64() << (op.rs32() & 0x3F), // DSLLV
                0x16 if h("dsrlv") => *op.mrd64() = op.rt64() >> (op.rs32() & 0x3F), // DSRLV
                0x17 if h("dsrav") => *op.mrd64() = (op.irt64() >> (op.rs32() & 0x3F)) as u64, // DSRAV
                0x18 if h("mult") => {
                    // MULT
                    let (hi, lo) =
                        (i64::wrapping_mul(op.rt32().isx64(), op.rs32().isx64()) as u64).hi_lo();
                    op.ctx.lo = lo;
                    op.ctx.hi = hi;
                }
                0x19 if h("multu") => {
                    // MULTU
                    let (hi, lo) = u64::wrapping_mul(op.rt32() as u64, op.rs32() as u64).hi_lo();
                    op.ctx.lo = lo;
                    op.ctx.hi = hi;
                }
                0x1A if h("div") => {
                    // DIV
                    op.ctx.lo = op.irs32().wrapping_div(op.irt32()).sx64();
                    op.ctx.hi = op.irs32().wrapping_rem(op.irt32()).sx64();
                }
                0x1B if h("divu") => {
                    // DIVU
                    op.ctx.lo = op.rs32().wrapping_div(op.rt32()).sx64();
                    op.ctx.hi = op.rs32().wrapping_rem(op.rt32()).sx64();
                }
                0x1C if h("dmult") => {
                    // DMULT
                    let (hi, lo) =
                        i128::wrapping_mul(op.irt64() as i128, op.irs64() as i128).hi_lo();
                    op.ctx.lo = lo as u64;
                    op.ctx.hi = hi as u64;
                }
                0x1D if h("dmultu") => {
                    // DMULTU
                    let (hi, lo) = u128::wrapping_mul(op.rt64() as u128, op.rs64() as u128).hi_lo();
                    op.ctx.lo = lo as u64;
                    op.ctx.hi = hi as u64;
                }
                0x1E if h("ddiv") => {
                    // DDIV
                    op.ctx.lo = op.irs64().wrapping_div(op.irt64()) as u64;
                    op.ctx.hi = op.irs64().wrapping_rem(op.irt64()) as u64;
                }
                0x1F if h("ddivu") => {
                    // DDIVU
                    op.ctx.lo = op.rs64().wrapping_div(op.rt64());
                    op.ctx.hi = op.rs64().wrapping_rem(op.rt64());
                }

                0x20 if h("add") => check_overflow_add!(op, *op.mrd64(), op.irs32(), op.irt32()), // ADD
                0x21 if h("addu") => *op.mrd64() = (op.rs32() + op.rt32()).sx64(), // ADDU
                0x22 if h("sub") => check_overflow_sub!(op, *op.mrd64(), op.irs32(), op.irt32()), // SUB
                0x23 if h("subu") => *op.mrd64() = (op.rs32() - op.rt32()).sx64(), // SUBU
                0x24 if h("and") => *op.mrd64() = op.rs64() & op.rt64(),           // AND
                0x25 if h("or") => *op.mrd64() = op.rs64() | op.rt64(),            // OR
                0x26 if h("xor") => *op.mrd64() = op.rs64() ^ op.rt64(),           // XOR
                0x27 if h("nor") => *op.mrd64() = !(op.rs64() | op.rt64()),        // NOR
                0x2A if h("slt") => *op.mrd64() = (op.irs32() < op.irt32()) as u64, // SLT
                0x2B if h("sltu") => *op.mrd64() = (op.rs32() < op.rt32()) as u64, // SLTU
                0x2C if h("dadd") => check_overflow_add!(op, *op.mrd64(), op.irs64(), op.irt64()), // DADD
                0x2D if h("daddu") => *op.mrd64() = op.rs64() + op.rt64(), // DADDU
                0x2E if h("dsub") => check_overflow_sub!(op, *op.mrd64(), op.irs64(), op.irt64()), // DSUB
                0x2F if h("dsubu") => *op.mrd64() = op.rs64() - op.rt64(), // DSUBU

                0x38 if h("dsll") => *op.mrd64() = op.rt64() << op.sa(), // DSLL
                0x3A if h("dsrl") => *op.mrd64() = op.rt64() >> op.sa(), // DSRL
                0x3B if h("dsra") => *op.mrd64() = (op.irt64() >> op.sa()) as u64, // DSRA
                0x3C if h("dsll32") => *op.mrd64() = op.rt64() << (op.sa() + 32), // DSLL32
                0x3E if h("dsrl32") => *op.mrd64() = op.rt64() >> (op.sa() + 32), // DSRL32
                0x3F if h("dsra32") => *op.mrd64() = (op.irt64() >> (op.sa() + 32)) as u64, // DSRA32

                _ => panic!("unimplemented special opcode: func=0x{:x?}", op.special()),
            },

            // REGIMM
            0x01 => match op.rt() {
                0x00 if h("bltz") => {
                    branch!(op, op.irs64() < 0, op.btgt(), link(false), likely(false))
                }
                0x01 if h("bgez") => {
                    branch!(op, op.irs64() >= 0, op.btgt(), link(false), likely(false))
                }
                0x02 if h("btlzl") => {
                    branch!(op, op.irs64() < 0, op.btgt(), link(false), likely(true))
                }
                0x03 if h("bgezl") => {
                    branch!(op, op.irs64() >= 0, op.btgt(), link(false), likely(true))
                }
                0x10 if h("bltzal") => {
                    branch!(op, op.irs64() < 0, op.btgt(), link(true), likely(false))
                }
                0x11 if h("bgezal") => {
                    branch!(op, op.irs64() >= 0, op.btgt(), link(true), likely(false))
                }
                0x12 if h("bltzall") => {
                    branch!(op, op.irs64() < 0, op.btgt(), link(true), likely(true))
                }
                0x13 if h("bgezall") => {
                    branch!(op, op.irs64() >= 0, op.btgt(), link(true), likely(true))
                }
                _ => panic!(
                    "unimplemented regimm opcode: func=0x{:x?} pc=0x{:x?}",
                    op.rt(),
                    op.ctx.pc - 4
                ),
            },

            0x02 if h("j") => branch!(op, true, op.jtgt(), link(false)), // J
            0x03 if h("jal") => branch!(op, true, op.jtgt(), link(true)), // JAL
            0x04 if h("beq") => branch!(op, op.rs64() == op.rt64(), op.btgt()), // BEQ
            0x05 if h("bne") => branch!(op, op.rs64() != op.rt64(), op.btgt()), // BNE
            0x06 if h("blez") => branch!(op, op.irs64() <= 0, op.btgt()), // BLEZ
            0x07 if h("bgtz") => branch!(op, op.irs64() > 0, op.btgt()), // BGTZ
            0x08 if h("addi") => check_overflow_add!(op, *op.mrt64(), op.irs32(), op.sximm32()), // ADDI
            0x09 if h("addiu") => *op.mrt64() = (op.irs32() + op.sximm32()).sx64(), // ADDIU
            0x0A if h("slti") => *op.mrt64() = (op.irs32() < op.sximm32()) as u64,  // SLTI
            0x0B if h("sltiu") => *op.mrt64() = (op.rs32() < op.sximm32() as u32) as u64, // SLTIU
            0x0C if h("andi") => *op.mrt64() = op.rs64() & op.imm64(),              // ANDI
            0x0D if h("ori") => *op.mrt64() = op.rs64() | op.imm64(),               // ORI
            0x0E if h("xori") => *op.mrt64() = op.rs64() ^ op.imm64(),              // XORI
            0x0F if h("lui") => *op.mrt64() = (op.sximm32() << 16).sx64(),          // LUI

            0x10 => if_cop!(op, cop0, { return cop0.op(&mut op.ctx, opcode, t) }), // COP0
            0x11 => if_cop!(op, cop1, { return cop1.op(&mut op.ctx, opcode, t) }), // COP1
            0x12 => if_cop!(op, cop2, { return cop2.op(&mut op.ctx, opcode, t) }), // COP2
            0x13 => if_cop!(op, cop3, { return cop3.op(&mut op.ctx, opcode, t) }), // COP3
            0x14 if h("beql") => branch!(op, op.rs64() == op.rt64(), op.btgt(), likely(true)), // BEQL
            0x15 if h("bnel") => branch!(op, op.rs64() != op.rt64(), op.btgt(), likely(true)), // BNEL
            0x16 if h("blezl") => branch!(op, op.irs64() <= 0, op.btgt(), likely(true)), // BLEZL
            0x17 if h("bgtzl") => branch!(op, op.irs64() > 0, op.btgt(), likely(true)),  // BGTZL
            0x18 if h("daddi") => check_overflow_add!(op, *op.mrt64(), op.irs64(), op.sximm64()), // DADDI
            0x19 if h("daddiu") => *op.mrt64() = (op.irs64() + op.sximm64()) as u64, // DADDIU
            0x1a if h("ldl") => *op.mrt64() = op.cpu.lwl::<u64>(op.ea(), op.rt64(), t)?, // LDL
            0x1b if h("ldr") => *op.mrt64() = op.cpu.lwr::<u64>(op.ea(), op.rt64(), t)?, // LDR

            0x20 if h("lb") => *op.mrt64() = op.cpu.read::<u8>(op.ea(), t)?.sx64(), // LB
            0x21 if h("lh") => *op.mrt64() = op.cpu.read::<u16>(op.ea(), t)?.sx64(), // LH
            0x22 if h("lwl") => *op.mrt64() = op.cpu.lwl::<u32>(op.ea(), op.rt32(), t)?.sx64(), // LWL
            0x23 if h("lw") => *op.mrt64() = op.cpu.read::<u32>(op.ea(), t)?.sx64(), // LW
            0x24 if h("lbu") => *op.mrt64() = op.cpu.read::<u8>(op.ea(), t)? as u64, // LBU
            0x25 if h("lhu") => *op.mrt64() = op.cpu.read::<u16>(op.ea(), t)? as u64, // LHU
            0x26 if h("lwr") => *op.mrt64() = op.cpu.lwr::<u32>(op.ea(), op.rt32(), t)?.sx64(), // LWR
            0x27 if h("lwu") => *op.mrt64() = op.cpu.read::<u32>(op.ea(), t)? as u64, // LWU
            0x28 if h("sb") => op.cpu.write::<u8>(op.ea(), op.rt32() as u8, t)?,      // SB
            0x29 if h("sh") => op.cpu.write::<u16>(op.ea(), op.rt32() as u16, t)?,    // SH
            0x2A if h("swl") => {
                // SWL
                op.cpu
                    .write::<u32>(op.ea(), op.cpu.swl(op.ea(), op.rt32(), t)?, t)?
            }
            0x2B if h("sw") => op.cpu.write::<u32>(op.ea(), op.rt32(), t)?, // SW
            0x2C if h("sdl") => {
                // SDL
                op.cpu
                    .write::<u64>(op.ea(), op.cpu.swl(op.ea(), op.rt64(), t)?, t)?
            }
            0x2D if h("sdr") => {
                // SDR
                op.cpu
                    .write::<u64>(op.ea(), op.cpu.swr(op.ea(), op.rt64(), t)?, t)?
            }
            0x2E if h("swr") => {
                // SWR
                op.cpu
                    .write::<u32>(op.ea(), op.cpu.swr(op.ea(), op.rt32(), t)?, t)?
            }
            0x2F => {} // CACHE

            0x31 if h("lwc1") => if_cop!(op, cop1, cop1.lwc(op.opcode, &mut op.ctx, &op.cpu.bus)), // LWC1
            0x32 if h("lwc2") => if_cop!(op, cop2, cop2.lwc(op.opcode, &mut op.ctx, &op.cpu.bus)), // LWC2
            0x35 if h("ldc1") => if_cop!(op, cop1, cop1.ldc(op.opcode, &mut op.ctx, &op.cpu.bus)), // LDC1
            0x36 if h("ldc2") => if_cop!(op, cop2, cop2.ldc(op.opcode, &mut op.ctx, &op.cpu.bus)), // LDC2
            0x37 if h("ld") => *op.mrt64() = op.cpu.read::<u64>(op.ea(), t)?, // LD
            0x39 if h("swc1") => if_cop!(op, cop1, cop1.swc(op.opcode, &op.ctx, &mut op.cpu.bus)), // SWC1
            0x3A if h("swc2") => if_cop!(op, cop2, cop2.swc(op.opcode, &op.ctx, &mut op.cpu.bus)), // SWC2
            0x3D if h("sdc1") => if_cop!(op, cop1, cop1.sdc(op.opcode, &op.ctx, &mut op.cpu.bus)), // SDC1
            0x3E if h("sdc2") => if_cop!(op, cop2, cop2.sdc(op.opcode, &op.ctx, &mut op.cpu.bus)), // SDC2
            0x3F if h("sd") => op.cpu.write::<u64>(op.ea(), op.rt64(), t)?, // SD

            _ => {
                panic!(
                    "unimplemented opcode: func=0x{:x?}, pc={}",
                    op.op(),
                    op.ctx.pc.hex()
                );
            }
        };
        Ok(())
    }

    fn lwl<S: MemInt>(&self, addr: u32, reg: S, t: &Tracer) -> Result<S> {
        let mem = self.read::<S>(addr, t)?;
        let shift = (addr as usize & (S::SIZE - 1)) << (S::SIZE_LOG + 1);
        let mask = S::truncate_from((1u64 << shift) - 1u64);
        Ok((reg & mask) | ((mem << shift) & !mask))
    }

    fn lwr<S: MemInt>(&self, addr: u32, reg: S, t: &Tracer) -> Result<S> {
        let mem = self.read::<S>(addr, t)?;
        let shift = (!addr as usize & (S::SIZE - 1)) << (S::SIZE_LOG + 1);
        let mask = S::max_value() >> shift;
        Ok((reg & !mask) | ((mem >> shift) & mask))
    }

    fn swl<S: MemInt>(&self, addr: u32, reg: S, t: &Tracer) -> Result<S> {
        let mem = self.read::<S>(addr, t)?;
        let shift = (addr as usize & (S::SIZE - 1)) << (S::SIZE_LOG + 1);
        let mask = S::max_value() >> shift;
        Ok((mem & !mask) | ((reg >> shift) & mask))
    }

    fn swr<S: MemInt>(&self, addr: u32, reg: S, t: &Tracer) -> Result<S> {
        let mem = self.read::<S>(addr, t)?;
        let shift = (!addr as usize & (S::SIZE - 1)) << (S::SIZE_LOG + 1);
        let mask = S::truncate_from((1 << shift) - 1);
        Ok((mem & mask) | ((reg << shift) & !mask))
    }

    // Check if the opcode has a visible effect on the CPU state, eg:
    // if it modifies any register. This is part of busy-wait detection.
    fn op_has_side_effects(&mut self, opcode: u32) -> bool {
        if opcode == 0 {
            // NOP
            return false;
        }
        match opcode >> 26 {
            0x20 | 0x21 | 0x22 | 0x23 | 0x24 | 0x25 | 0x26 | 0x27 => {
                // Load opcode. Check if the address is raw memory.
                let sximm32 = (opcode & 0xffff) as i16 as i32;
                let rs = ((opcode >> 21) & 0x1f) as usize;
                let ea = self.ctx.regs[rs] as u32 + sximm32 as u32;
                let mem = self.bus.fetch_read_nolog::<u32>(C::addr_mask(ea));
                return !mem.is_mem();
            }
            0x28 | 0x29 | 0x2A | 0x2B | 0x2E => {
                // Store opcode. Check if the address is raw memory.
                let sximm32 = (opcode & 0xffff) as i16 as i32;
                let rs = ((opcode >> 21) & 0x1f) as usize;
                let ea = self.ctx.regs[rs] as u32 + sximm32 as u32;
                let mem = self.bus.fetch_write_nolog::<u32>(C::addr_mask(ea));
                return !mem.is_mem();
            }
            _ => return true,
        }
    }

    // Check if a 2-insn loop is a busy wait. If the delay slot has no visible
    // effect on the CPU state, then it is a busy-wait and can be skipped.
    fn detect_busywait_2insn(&mut self, branch_pc: u64) {
        if self.last_busycheck == branch_pc {
            return;
        }
        self.last_busycheck = branch_pc;
        let opcode_after = self.fetch(branch_pc + 4).read();
        let busywait = !self.op_has_side_effects(opcode_after);
        if busywait {
            // Skip busy-wait by short-circuiting the interpret loop until
            // the end of our timeslice.
            self.ctx.clock = self.until;
            self.last_busycheck = 0;
        }
    }

    // Check if a 3-insn loop is a busy wait. If the delay slot and the
    // instruction before the branch have no visible effect on the CPU state,
    // then it is a busy-wait and can be skipped.
    fn detect_busywait_3insn(&mut self, branch_pc: u64) {
        if self.last_busycheck == branch_pc {
            return;
        }
        self.last_busycheck = branch_pc;
        let opcode_before = self.fetch(branch_pc - 4).read();
        let opcode_after = self.fetch(branch_pc + 4).read();
        let busywait =
            !self.op_has_side_effects(opcode_before) && !self.op_has_side_effects(opcode_after);
        if busywait {
            // Skip busy-wait by short-circuiting the interpret loop until
            // the end of our timeslice.
            self.ctx.clock = self.until;
            self.last_busycheck = 0;
        }
    }

    fn fetch(&mut self, addr: u64) -> MemIoR<u32> {
        self.bus.fetch_read::<u32>(C::pc_mask(addr as u32))
    }

    fn read<U: MemInt>(&self, addr: u32, t: &Tracer) -> Result<U> {
        let val = self
            .bus
            .read::<U>(C::addr_mask(addr) & !(U::SIZE as u32 - 1));
        t.trace_mem_read(&self.name, addr.into(), U::ACCESS_SIZE, val.into())?;
        Ok(val)
    }

    fn write<U: MemInt>(&mut self, addr: u32, val: U, t: &Tracer) -> Result<()> {
        self.bus
            .write::<U>(C::addr_mask(addr) & !(U::SIZE as u32 - 1), val);
        t.trace_mem_write(&self.name, addr.into(), U::ACCESS_SIZE, val.into())
    }

    pub fn run(&mut self, until: i64, t: &Tracer) -> Result<()> {
        self.until = until;

        let ctx = unsafe { self.ctx.as_mut() };
        let mut mem = self.fetch(ctx.pc);
        let mut last_mem_pc = ctx.pc;

        while ctx.clock < self.until {
            if ctx.lines.halt {
                ctx.clock = self.until;
                return Ok(());
            }

            // See if there are pending interrupts that COP0 can generate.
            self.cop0.poll_interrupts(ctx);

            // Fetch the next memory area (unless we're looping, in which case
            // we already have the memory pointer).
            if ctx.pc != last_mem_pc {
                mem = self.fetch(ctx.pc);
                last_mem_pc = ctx.pc;
            }

            let mut iter = mem
                .iter()
                .unwrap_or_else(|| panic!("jumped to non-linear memory: {}", ctx.pc.hex()));

            // Tight loop: go through continuous memory, no branches, no IRQs
            while let Some(op) = iter.next() {
                ctx.tight_exit = ctx.delay_slot;
                ctx.delay_slot = false;
                ctx.pc = ctx.next_pc;
                ctx.next_pc += 4;
                self.op(ctx, op, t)?;
                t.trace_insn(&self.name, C::pc_mask(ctx.pc as u32) as u64)?;
                if ctx.clock >= self.until || ctx.tight_exit {
                    break;
                }
            }
        }
        Ok(())
    }
}

impl<C: Config> sync::Subsystem for Cpu<C> {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, until: i64, tracer: &Tracer) -> Result<()> {
        Cpu::run(self, until, tracer)
    }

    fn step(&mut self, tracer: &Tracer) -> Result<()> {
        Cpu::run(self, self.ctx.clock + 1, tracer)
    }

    fn cycles(&self) -> i64 {
        self.ctx.clock
    }

    fn pc(&self) -> Option<u64> {
        Some(self.ctx.pc)
    }
}

impl<C: Config> Cpu<C> {
    pub fn render_debug<'a, 'ui>(&mut self, dr: &DebuggerRenderer<'a, 'ui>) {
        dr.render_disasmview(self);
        dr.render_regview(self);

        if !self.cop0.is_null_obj() {
            self.cop0.render_debug(dr);
        }
        if !self.cop1.is_null_obj() {
            self.cop1.render_debug(dr);
        }
        if !self.cop2.is_null_obj() {
            self.cop1.render_debug(dr);
        }
        if !self.cop3.is_null_obj() {
            self.cop1.render_debug(dr);
        }
    }
}

impl<C: Config> RegisterView for Cpu<C> {
    const WINDOW_SIZE: (f32, f32) = (380.0, 400.0);
    const COLUMNS: usize = 3;

    fn name(&self) -> &str {
        &self.name
    }

    fn visit_regs<'s, F>(&'s mut self, col: usize, mut visit: F)
    where
        F: for<'a> FnMut(&'a str, RegisterSize<'a>, Option<&str>),
    {
        use self::RegisterSize::*;
        match col {
            0 | 1 => {
                let regs = vec![
                    "zr", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4",
                    "t5", "t6", "t7", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9",
                    "k0", "k1", "gp", "sp", "fp", "ra",
                ];
                for (n, v) in regs.iter().zip(&mut self.ctx.regs).skip(col * 16).take(16) {
                    visit(n, Reg64(v), None);
                }
            }
            2 => {
                visit("hi", Reg64(&mut self.ctx.hi), None);
                visit("lo", Reg64(&mut self.ctx.lo), None);

                let mut pcdesc = format!("DelaySlot:{}", self.ctx.delay_slot);
                if self.ctx.delay_slot {
                    pcdesc += &format!("\nJumpTo:{:x}", self.ctx.next_pc);
                }
                visit("pc", Reg64(&mut self.ctx.pc), Some(&pcdesc));
            }
            _ => unreachable!(),
        };
    }
}

impl<C: Config> DisasmView for Cpu<C> {
    fn name(&self) -> &str {
        &self.name
    }

    fn pc(&self) -> u64 {
        C::pc_mask(self.ctx.pc as u32).into()
    }

    fn pc_range(&self) -> (u64, u64) {
        (C::pc_mask(0x0).into(), C::pc_mask(0xFFFF_FFFF).into())
    }

    fn disasm_block<Func: FnMut(u64, &[u8], &str)>(&self, pc_range: (u64, u64), mut f: Func) {
        let mut buf = vec![0u8, 0u8, 0u8, 0u8];
        let mut pc = pc_range.0 as u32;

        let mut dis = move |pc: u32, opcode: u32| {
            byteorder::BigEndian::write_u32(&mut buf, opcode);
            let insn = decode(self, opcode, pc.into()).disasm();
            f(pc as u64, &buf, &insn);
        };

        while pc < pc_range.1 as u32 {
            let mem = self
                .bus
                .fetch_read_nolog::<u32>(C::pc_mask(pc.into()) as u32);

            let iter = mem.iter();
            if iter.is_none() {
                dis(pc, mem.read());
                pc += 4;
                continue;
            }
            let mut iter = iter.unwrap();

            while pc < pc_range.1 as u32 {
                match iter.next() {
                    Some(opcode) => dis(pc, opcode),
                    None => break,
                };
                pc += 4;
            }
        }
    }
}
