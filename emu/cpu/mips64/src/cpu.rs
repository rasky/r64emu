use super::decode::{decode, DecodedInsn};
use super::mmu::Mmu;

use emu::bus::be::{Bus, MemIoR};
use emu::bus::MemInt;
use emu::dbg::{DebuggerRenderer, DisasmView, RegisterSize, RegisterView, Result, Tracer};
use emu::int::Numerics;
use emu::sync;

use byteorder::ByteOrder;
use slog;

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

/// Cop is a MIPS64 coprocessor that can be installed within the core.
pub trait Cop {
    fn reg(&self, idx: usize) -> u128;
    fn set_reg(&mut self, idx: usize, val: u128);

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32, t: &Tracer) -> Result<()>;
    fn decode(&self, _opcode: u32, _pc: u64) -> DecodedInsn {
        DecodedInsn::new0("unkcop")
    }

    fn lwc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = bus.borrow().read::<u32>(ea & 0x1FFF_FFFC) as u64;
        self.set_reg(rt, val as u128);
    }

    fn ldc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = bus.borrow().read::<u64>(ea & 0x1FFF_FFFC) as u64;
        self.set_reg(rt, val as u128);
    }

    fn swc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = self.reg(rt) as u32;
        bus.borrow().write::<u32>(ea & 0x1FFF_FFFC, val);
    }

    fn sdc(&mut self, op: u32, ctx: &CpuContext, bus: &Rc<RefCell<Box<Bus>>>) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = self.reg(rt) as u64;
        bus.borrow().write::<u64>(ea & 0x1FFF_FFFC, val);
    }

    // Implement some debugger views
    fn render_debug<'a, 'ui>(&mut self, _dr: &DebuggerRenderer<'a, 'ui>) {}
}

#[derive(Copy, Clone, Debug)]
pub enum Exception {
    Interrupt,  // Interrupt
    Breakpoint, // Breakpoint
    Reset,
    Nmi,
    SoftReset,
    TlbRefill,
    XTlbRefill,
}

impl Exception {
    pub(crate) fn exc_code(&self) -> Option<u32> {
        use self::Exception::*;
        match self {
            Interrupt => Some(0x00),
            Breakpoint => Some(0x09),
            Reset => None,
            Nmi => None,
            SoftReset => None,
            TlbRefill => None,
            XTlbRefill => None,
        }
    }
}

struct Lines {
    halt: bool,
}

/// Cop0 is a MIPS64 coprocessor #0, which (in addition to being a normal coprocessor)
/// it is able to control execution of the core by triggering exceptions.
pub trait Cop0: Cop {
    // Change a single external interrupt line.
    // Notice that hwint line 0 is mapped to bit IP2 in Cause
    // (because IP0/IP1 are used for software interrupts).
    fn set_hwint_line(&mut self, line: usize, status: bool);

    /// Check if there's a pending interrupt. It is expected that if this
    /// function returns true, Cop0::exception() is immediately called with
    /// exc == Exception::Int.
    fn pending_int(&self) -> bool;

    /// Trigger the specified excepion.
    fn exception(&mut self, ctx: &mut CpuContext, exc: Exception);
}

pub struct CpuContext {
    pub regs: [u64; 32],
    pub hi: u64,
    pub lo: u64,
    pub(crate) pc: u64,
    pub(crate) next_pc: u64,
    pub clock: i64,
    pub tight_exit: bool,
    pub delay_slot: bool,
    pub mmu: Mmu,
    lines: Lines,
}

pub struct Cpu {
    ctx: CpuContext,

    cop0: Option<Rc<RefCell<Box<Cop0>>>>,
    cop1: Option<Box<Cop>>,
    cop2: Option<Box<Cop>>,
    cop3: Option<Box<Cop>>,

    name: String,
    logger: slog::Logger,
    bus: Rc<RefCell<Box<Bus>>>,
    until: i64,

    last_fetch_addr: u32,
    last_fetch_mem: MemIoR<u32>,

    // Masks that isolate lines that are connected to the external bus
    // Defaults to 0x1FFF_FFFF for all three accesses types.
    pub bus_read_mask: u32,
    pub bus_write_mask: u32,
    pub bus_fetch_mask: u32,
    pub bus_fetch_fixed: u32,
}

struct Mipsop<'a> {
    opcode: u32,
    cpu: &'a mut Cpu,
}

impl<'a> Mipsop<'a> {
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
        self.cpu.ctx.pc + self.sximm64() as u64 * 4
    }
    fn jtgt(&self) -> u64 {
        (self.cpu.ctx.pc & 0xFFFF_FFFF_F000_0000) + ((self.opcode & 0x03FF_FFFF) * 4) as u64
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
        self.cpu.ctx.regs[self.rs()]
    }
    fn rt64(&self) -> u64 {
        self.cpu.ctx.regs[self.rt()]
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
        &mut self.cpu.ctx.regs[self.rt()]
    }
    fn mrd64(&'a mut self) -> &'a mut u64 {
        &mut self.cpu.ctx.regs[self.rd()]
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
            $op.cpu.ctx.regs[31] = $op.cpu.ctx.pc + 4;
        }
        let (cond, tgt) = ($cond, $tgt);
        $op.cpu.ctx.branch(cond, tgt, $lkl);
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
        match $op.cpu.$cop {
            Some(ref mut $cop) => $do,
            None => {
                let pc = $op.cpu.ctx.pc;
                let opcode = $op.opcode;
                warn!($op.cpu.logger, "COP opcode without COP"; o!("pc" => pc.hex(), "op" => opcode.hex()));
            }
        }
    }};
}

impl Cpu {
    pub fn new(name: &str, logger: slog::Logger, bus: Rc<RefCell<Box<Bus>>>) -> Cpu {
        let reset_vector = 0xFFFF_FFFF_BFC0_0000; // FIXME
        return Cpu {
            ctx: CpuContext {
                regs: [0u64; 32],
                hi: 0,
                lo: 0,
                pc: reset_vector,
                next_pc: reset_vector + 4,
                clock: 0,
                tight_exit: false,
                delay_slot: false,
                lines: Lines { halt: false },
                mmu: Mmu::default(),
            },
            bus: bus,
            name: name.into(),
            cop0: None,
            cop1: None,
            cop2: None,
            cop3: None,
            logger: logger,
            until: 0,
            last_fetch_addr: 0xFFFF_FFFF,
            last_fetch_mem: MemIoR::default(),
            bus_read_mask: 0x1FFF_FFFF,
            bus_write_mask: 0x1FFF_FFFF,
            bus_fetch_mask: 0x1FFF_FFFF,
            bus_fetch_fixed: 0,
        };
    }

    pub fn ctx(&self) -> &CpuContext {
        &self.ctx
    }

    pub fn ctx_mut(&mut self) -> &mut CpuContext {
        &mut self.ctx
    }

    pub fn set_cop0(&mut self, cop0: Box<dyn Cop0>) {
        self.cop0 = Some(Rc::new(RefCell::new(cop0)));
    }
    pub fn set_cop1(&mut self, cop1: Box<dyn Cop>) {
        self.cop1 = Some(cop1);
    }
    pub fn set_cop2(&mut self, cop2: Box<dyn Cop>) {
        self.cop2 = Some(cop2);
    }
    pub fn set_cop3(&mut self, cop3: Box<dyn Cop>) {
        self.cop3 = Some(cop3);
    }

    pub fn cop0(&self) -> Option<Ref<Box<dyn Cop0>>> {
        self.cop0.as_ref().map(|c| c.borrow())
    }
    pub fn cop1(&self) -> Option<&Box<dyn Cop>> {
        self.cop1.as_ref()
    }
    pub fn cop2(&self) -> Option<&Box<dyn Cop>> {
        self.cop2.as_ref()
    }
    pub fn cop3(&self) -> Option<&Box<dyn Cop>> {
        self.cop3.as_ref()
    }

    pub fn cop0_mut(&mut self) -> Option<RefMut<Box<dyn Cop0>>> {
        self.cop0.as_ref().map(|c| c.borrow_mut())
    }
    pub fn cop1_mut(&mut self) -> Option<&mut Box<dyn Cop>> {
        self.cop1.as_mut()
    }
    pub fn cop2_mut(&mut self) -> Option<&mut Box<dyn Cop>> {
        self.cop2.as_mut()
    }
    pub fn cop3_mut(&mut self) -> Option<&mut Box<dyn Cop>> {
        self.cop3.as_mut()
    }

    pub fn cop0_clone(&self) -> Option<Rc<RefCell<Box<dyn Cop0>>>> {
        self.cop0.as_ref().map(|c| c.clone())
    }

    pub fn reset(&mut self) {
        self.exception(Exception::Reset);
    }

    fn exception(&mut self, exc: Exception) {
        if let Some(ref mut cop0) = self.cop0 {
            cop0.borrow_mut().exception(&mut self.ctx, exc);
        }
    }

    fn trap_overflow(&mut self) {
        unimplemented!();
    }

    #[inline(always)]
    fn op(&mut self, opcode: u32, t: &Tracer) -> Result<()> {
        self.ctx.clock += 1;
        let mut op = Mipsop { opcode, cpu: self };
        match op.op() {
            // SPECIAL
            0x00 => match op.special() {
                0x00 => *op.mrd64() = (op.rt32() << op.sa()).sx64(), // SLL
                0x02 => *op.mrd64() = (op.rt32() >> op.sa()).sx64(), // SRL
                0x03 => *op.mrd64() = (op.irt32() >> op.sa()).sx64(), // SRA
                0x04 => *op.mrd64() = (op.rt32() << (op.rs32() & 0x1F)).sx64(), // SLLV
                0x06 => *op.mrd64() = (op.rt32() >> (op.rs32() & 0x1F)).sx64(), // SRLV
                0x07 => *op.mrd64() = (op.irt32() >> (op.rs32() & 0x1F)).sx64(), // SRAV
                0x08 => branch!(op, true, op.rs64(), link(false)),   // JR
                0x09 => branch!(op, true, op.rs64(), link(true)),    // JALR
                0x0D => op.cpu.exception(Exception::Breakpoint),     // BREAK
                0x0F => {}                                           // SYNC

                0x10 => *op.mrd64() = op.cpu.ctx.hi, // MFHI
                0x11 => op.cpu.ctx.hi = op.rs64(),   // MTHI
                0x12 => *op.mrd64() = op.cpu.ctx.lo, // MFLO
                0x13 => op.cpu.ctx.lo = op.rs64(),   // MTLO
                0x14 => *op.mrd64() = op.rt64() << (op.rs32() & 0x3F), // DSLLV
                0x16 => *op.mrd64() = op.rt64() >> (op.rs32() & 0x3F), // DSRLV
                0x17 => *op.mrd64() = (op.irt64() >> (op.rs32() & 0x3F)) as u64, // DSRAV
                0x18 => {
                    // MULT
                    let (hi, lo) =
                        (i64::wrapping_mul(op.rt32().isx64(), op.rs32().isx64()) as u64).hi_lo();
                    op.cpu.ctx.lo = lo;
                    op.cpu.ctx.hi = hi;
                }
                0x19 => {
                    // MULTU
                    let (hi, lo) = u64::wrapping_mul(op.rt32() as u64, op.rs32() as u64).hi_lo();
                    op.cpu.ctx.lo = lo;
                    op.cpu.ctx.hi = hi;
                }
                0x1A => {
                    // DIV
                    op.cpu.ctx.lo = op.irs32().wrapping_div(op.irt32()).sx64();
                    op.cpu.ctx.hi = op.irs32().wrapping_rem(op.irt32()).sx64();
                }
                0x1B => {
                    // DIVU
                    op.cpu.ctx.lo = op.rs32().wrapping_div(op.rt32()).sx64();
                    op.cpu.ctx.hi = op.rs32().wrapping_rem(op.rt32()).sx64();
                }
                0x1C => {
                    // DMULT
                    let (hi, lo) =
                        i128::wrapping_mul(op.irt64() as i128, op.irs64() as i128).hi_lo();
                    op.cpu.ctx.lo = lo as u64;
                    op.cpu.ctx.hi = hi as u64;
                }
                0x1D => {
                    // DMULTU
                    let (hi, lo) = u128::wrapping_mul(op.rt64() as u128, op.rs64() as u128).hi_lo();
                    op.cpu.ctx.lo = lo as u64;
                    op.cpu.ctx.hi = hi as u64;
                }
                0x1E => {
                    // DDIV
                    op.cpu.ctx.lo = op.irs64().wrapping_div(op.irt64()) as u64;
                    op.cpu.ctx.hi = op.irs64().wrapping_rem(op.irt64()) as u64;
                }
                0x1F => {
                    // DDIVU
                    op.cpu.ctx.lo = op.rs64().wrapping_div(op.rt64());
                    op.cpu.ctx.hi = op.rs64().wrapping_rem(op.rt64());
                }

                0x20 => check_overflow_add!(op, *op.mrd64(), op.irs32(), op.irt32()), // ADD
                0x21 => *op.mrd64() = (op.rs32() + op.rt32()).sx64(),                 // ADDU
                0x22 => check_overflow_sub!(op, *op.mrd64(), op.irs32(), op.irt32()), // SUB
                0x23 => *op.mrd64() = (op.rs32() - op.rt32()).sx64(),                 // SUBU
                0x24 => *op.mrd64() = op.rs64() & op.rt64(),                          // AND
                0x25 => *op.mrd64() = op.rs64() | op.rt64(),                          // OR
                0x26 => *op.mrd64() = op.rs64() ^ op.rt64(),                          // XOR
                0x27 => *op.mrd64() = !(op.rs64() | op.rt64()),                       // NOR
                0x2A => *op.mrd64() = (op.irs32() < op.irt32()) as u64,               // SLT
                0x2B => *op.mrd64() = (op.rs32() < op.rt32()) as u64,                 // SLTU
                0x2C => check_overflow_add!(op, *op.mrd64(), op.irs64(), op.irt64()), // DADD
                0x2D => *op.mrd64() = op.rs64() + op.rt64(),                          // DADDU
                0x2E => check_overflow_sub!(op, *op.mrd64(), op.irs64(), op.irt64()), // DSUB
                0x2F => *op.mrd64() = op.rs64() - op.rt64(),                          // DSUBU

                0x38 => *op.mrd64() = op.rt64() << op.sa(), // DSLL
                0x3A => *op.mrd64() = op.rt64() >> op.sa(), // DSRL
                0x3B => *op.mrd64() = (op.irt64() >> op.sa()) as u64, // DSRA
                0x3C => *op.mrd64() = op.rt64() << (op.sa() + 32), // DSLL32
                0x3E => *op.mrd64() = op.rt64() >> (op.sa() + 32), // DSRL32
                0x3F => *op.mrd64() = (op.irt64() >> (op.sa() + 32)) as u64, // DSRA32

                _ => panic!("unimplemented special opcode: func=0x{:x?}", op.special()),
            },

            // REGIMM
            0x01 => match op.rt() {
                0x00 => branch!(op, op.irs64() < 0, op.btgt(), link(false), likely(false)), // BLTZ
                0x01 => branch!(op, op.irs64() >= 0, op.btgt(), link(false), likely(false)), // BGEZ
                0x02 => branch!(op, op.irs64() < 0, op.btgt(), link(false), likely(true)),  // BLTZL
                0x03 => branch!(op, op.irs64() >= 0, op.btgt(), link(false), likely(true)), // BGEZL
                0x10 => branch!(op, op.irs64() < 0, op.btgt(), link(true), likely(false)), // BLTZAL
                0x11 => branch!(op, op.irs64() >= 0, op.btgt(), link(true), likely(false)), // BGEZAL
                0x12 => branch!(op, op.irs64() < 0, op.btgt(), link(true), likely(true)), // BLTZALL
                0x13 => branch!(op, op.irs64() >= 0, op.btgt(), link(true), likely(true)), // BGEZALL
                _ => panic!(
                    "unimplemented regimm opcode: func=0x{:x?} pc=0x{:x?}",
                    op.rt(),
                    op.cpu.ctx.pc - 4
                ),
            },

            0x02 => branch!(op, true, op.jtgt(), link(false)), // J
            0x03 => branch!(op, true, op.jtgt(), link(true)),  // JAL
            0x04 => branch!(op, op.rs64() == op.rt64(), op.btgt()), // BEQ
            0x05 => branch!(op, op.rs64() != op.rt64(), op.btgt()), // BNE
            0x06 => branch!(op, op.irs64() <= 0, op.btgt()),   // BLEZ
            0x07 => branch!(op, op.irs64() > 0, op.btgt()),    // BGTZ
            0x08 => check_overflow_add!(op, *op.mrt64(), op.irs32(), op.sximm32()), // ADDI
            0x09 => *op.mrt64() = (op.irs32() + op.sximm32()).sx64(), // ADDIU
            0x0A => *op.mrt64() = (op.irs32() < op.sximm32()) as u64, // SLTI
            0x0B => *op.mrt64() = (op.rs32() < op.sximm32() as u32) as u64, // SLTIU
            0x0C => *op.mrt64() = op.rs64() & op.imm64(),      // ANDI
            0x0D => *op.mrt64() = op.rs64() | op.imm64(),      // ORI
            0x0E => *op.mrt64() = op.rs64() ^ op.imm64(),      // XORI
            0x0F => *op.mrt64() = (op.sximm32() << 16).sx64(), // LUI

            0x10 => if_cop!(op, cop0, {
                return cop0.borrow_mut().op(&mut op.cpu.ctx, opcode, t);
            }), // COP0
            0x11 => if_cop!(op, cop1, { return cop1.op(&mut op.cpu.ctx, opcode, t) }), // COP1
            0x12 => if_cop!(op, cop2, { return cop2.op(&mut op.cpu.ctx, opcode, t) }), // COP2
            0x13 => if_cop!(op, cop3, { return cop3.op(&mut op.cpu.ctx, opcode, t) }), // COP3
            0x14 => branch!(op, op.rs64() == op.rt64(), op.btgt(), likely(true)),      // BEQL
            0x15 => branch!(op, op.rs64() != op.rt64(), op.btgt(), likely(true)),      // BNEL
            0x16 => branch!(op, op.irs64() <= 0, op.btgt(), likely(true)),             // BLEZL
            0x17 => branch!(op, op.irs64() > 0, op.btgt(), likely(true)),              // BGTZL
            0x18 => check_overflow_add!(op, *op.mrt64(), op.irs64(), op.sximm64()),    // DADDI
            0x19 => *op.mrt64() = (op.irs64() + op.sximm64()) as u64,                  // DADDIU

            0x20 => *op.mrt64() = op.cpu.read::<u8>(op.ea(), t)?.sx64(), // LB
            0x21 => *op.mrt64() = op.cpu.read::<u16>(op.ea(), t)?.sx64(), // LH
            0x22 => *op.mrt64() = op.cpu.lwl(op.ea(), op.rt32(), t)?.sx64(), // LWL
            0x23 => *op.mrt64() = op.cpu.read::<u32>(op.ea(), t)?.sx64(), // LW
            0x24 => *op.mrt64() = op.cpu.read::<u8>(op.ea(), t)? as u64, // LBU
            0x25 => *op.mrt64() = op.cpu.read::<u16>(op.ea(), t)? as u64, // LHU
            0x26 => *op.mrt64() = op.cpu.lwr(op.ea(), op.rt32(), t)?.sx64(), // LWR
            0x27 => *op.mrt64() = op.cpu.read::<u32>(op.ea(), t)? as u64, // LWU
            0x28 => op.cpu.write::<u8>(op.ea(), op.rt32() as u8, t)?,    // SB
            0x29 => op.cpu.write::<u16>(op.ea(), op.rt32() as u16, t)?,  // SH
            0x2A => op
                .cpu
                .write::<u32>(op.ea(), op.cpu.swl(op.ea(), op.rt32(), t)?, t)?, // SWL
            0x2B => op.cpu.write::<u32>(op.ea(), op.rt32(), t)?,         // SW
            0x2E => op
                .cpu
                .write::<u32>(op.ea(), op.cpu.swr(op.ea(), op.rt32(), t)?, t)?, // SWR
            0x2F => {}                                                   // CACHE

            0x31 => if_cop!(op, cop1, cop1.lwc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // LWC1
            0x32 => if_cop!(op, cop2, cop2.lwc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // LWC2
            0x35 => if_cop!(op, cop1, cop1.ldc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // LDC1
            0x36 => if_cop!(op, cop2, cop2.ldc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // LDC2
            0x37 => *op.mrt64() = op.cpu.read::<u64>(op.ea(), t)?,                    // LD
            0x39 => if_cop!(op, cop1, cop1.swc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // SWC1
            0x3A => if_cop!(op, cop2, cop2.swc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // SWC2
            0x3D => if_cop!(op, cop1, cop1.sdc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // SDC1
            0x3E => if_cop!(op, cop2, cop2.sdc(op.opcode, &op.cpu.ctx, &op.cpu.bus)), // SDC2
            0x3F => op.cpu.write::<u64>(op.ea(), op.rt64(), t)?,                      // SD

            _ => {
                panic!(
                    "unimplemented opcode: func=0x{:x?}, pc={}",
                    op.op(),
                    op.cpu.ctx.pc.hex()
                );
            }
        };
        Ok(())
    }

    fn lwl(&self, addr: u32, reg: u32, t: &Tracer) -> Result<u32> {
        let mem = self.read::<u32>(addr, t)?;
        let shift = (addr & 3) * 8;
        let mask = (1 << shift) - 1;
        Ok((reg & mask) | ((mem << shift) & !mask))
    }

    fn lwr(&self, addr: u32, reg: u32, t: &Tracer) -> Result<u32> {
        let mem = self.read::<u32>(addr, t)?;
        let shift = (!addr & 3) * 8;
        let mask = ((1u64 << (32 - shift)) - 1) as u32;
        Ok((reg & !mask) | ((mem >> shift) & mask))
    }

    fn swl(&self, addr: u32, reg: u32, t: &Tracer) -> Result<u32> {
        let mem = self.read::<u32>(addr, t)?;
        let shift = (addr & 3) * 8;
        let mask = ((1u64 << (32 - shift)) - 1) as u32;
        Ok((mem & !mask) | ((reg >> shift) & mask))
    }

    fn swr(&self, addr: u32, reg: u32, t: &Tracer) -> Result<u32> {
        let mem = self.read::<u32>(addr, t)?;
        let shift = (!addr & 3) * 8;
        let mask = (1 << shift) - 1;
        Ok((mem & mask) | ((reg << shift) & !mask))
    }

    fn pc_fetch_mask(&self, pc: u64) -> u64 {
        let pc = pc as u32;
        ((pc & 0xFFFF_FFFC & self.bus_fetch_mask) | self.bus_fetch_fixed) as u64
    }

    fn fetch(&mut self, addr: u64) -> &MemIoR<u32> {
        // Save last fetched memio, to speed up hot loops
        let addr = addr as u32;
        if self.last_fetch_addr != addr {
            self.last_fetch_addr = addr;
            self.last_fetch_mem = self
                .bus
                .borrow()
                .fetch_read::<u32>(self.pc_fetch_mask(addr as u64) as u32);
        }
        &self.last_fetch_mem
    }

    fn read<U: MemInt>(&self, addr: u32, t: &Tracer) -> Result<U> {
        let val = self
            .bus
            .borrow()
            .read::<U>(addr & self.bus_read_mask & !(U::SIZE as u32 - 1));
        t.trace_mem_read(&self.name, addr.into(), U::ACCESS_SIZE, val.into())?;
        Ok(val)
    }

    fn write<U: MemInt>(&self, addr: u32, val: U, t: &Tracer) -> Result<()> {
        self.bus
            .borrow()
            .write::<U>(addr & self.bus_write_mask & !(U::SIZE as u32 - 1), val);
        t.trace_mem_write(&self.name, addr.into(), U::ACCESS_SIZE, val.into())
    }

    pub fn run(&mut self, until: i64, t: &Tracer) -> Result<()> {
        self.until = until;
        while self.ctx.clock < self.until {
            if self.ctx.lines.halt {
                self.ctx.clock = self.until;
                return Ok(());
            }

            if let Some(ref mut cop0) = self.cop0 {
                let mut cop0 = cop0.borrow_mut();
                if cop0.pending_int() {
                    cop0.exception(&mut self.ctx, Exception::Interrupt);
                    continue;
                }
            }

            let mut iter = self
                .fetch(self.ctx.pc)
                .iter()
                .unwrap_or_else(|| panic!("jumped to non-linear memory: {}", self.ctx.pc.hex()));

            // Tight loop: go through continuous memory, no branches, no IRQs
            while let Some(op) = iter.next() {
                self.ctx.tight_exit = self.ctx.delay_slot;
                self.ctx.delay_slot = false;
                self.ctx.pc = self.ctx.next_pc;
                self.ctx.next_pc += 4;
                self.op(op, t)?;
                t.trace_insn(&self.name, self.pc_fetch_mask(self.ctx.pc))?;
                if self.ctx.clock >= self.until || self.ctx.tight_exit {
                    break;
                }
            }
        }
        Ok(())
    }
}

impl sync::Subsystem for Box<Cpu> {
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

impl Cpu {
    pub fn render_debug<'a, 'ui>(self: &mut Box<Cpu>, dr: &DebuggerRenderer<'a, 'ui>) {
        dr.render_disasmview(self);
        dr.render_regview(self);

        match self.cop0_mut() {
            Some(mut c) => c.render_debug(dr),
            None => {}
        };
        match self.cop1_mut() {
            Some(c) => c.render_debug(dr),
            None => {}
        };
        match self.cop2_mut() {
            Some(c) => c.render_debug(dr),
            None => {}
        };
        match self.cop3_mut() {
            Some(c) => c.render_debug(dr),
            None => {}
        };
    }
}

impl RegisterView for Box<Cpu> {
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

impl DisasmView for Box<Cpu> {
    fn name(&self) -> &str {
        &self.name
    }

    fn pc(&self) -> u64 {
        self.pc_fetch_mask(self.ctx.pc)
    }

    fn pc_range(&self) -> (u64, u64) {
        (
            self.pc_fetch_mask(0x0),
            self.pc_fetch_mask(0xFFFF_FFFF_FFFF_FFFF),
        )
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
                .borrow()
                .fetch_read_nolog::<u32>(self.pc_fetch_mask(pc.into()) as u32);

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
