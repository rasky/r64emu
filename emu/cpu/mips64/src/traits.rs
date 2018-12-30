use super::{CpuContext, DecodedInsn, Exception};
use emu::bus::be::Bus;
use emu::dbg::{DebuggerRenderer, Result, Tracer};

/// Arch is a trait that allows to customise the MIPS core at the opcode level.
/// It is used to implement different MIPS variants (architecture levels).
pub trait Arch {
    // Returns whether the specified opcode is implemented in this architecture.
    // This is meant to be called always with literals and is expected to inline
    // and produce a compile-time boolean flag, which actually removes the
    // the opcode from the implementation.
    #[inline(always)]
    fn has_op(op: &'static str) -> bool;
}

/// Config is a trait that allows to describe the MIPS hardware-level configuration.
/// It specifies the architecture, the available coprocessors, and the bus
/// accesses.
pub trait Config {
    type Arch: Arch;

    /// Coprocessors. If not bound, use CopNull.
    type Cop0: Cop0;
    type Cop1: Cop;
    type Cop2: Cop;
    type Cop3: Cop;

    // Mask PC before fetching from the bus. This should be reimplemented
    // by architectures that do not have a full 64-bit bus to simplify
    // bus mapping.
    fn pc_mask(pc: u32) -> u32 {
        pc & 0x1FFF_FFFF
    }

    // Mask addresses before reading/writing from the bus. This should be reimplemented
    // by architectures that do not have a full 64-bit bus to simplify
    // bus mapping.
    fn addr_mask(addr: u32) -> u32 {
        addr & 0x1FFF_FFFF
    }
}

/// Cop is a MIPS64 coprocessor that can be installed within the core.
pub trait Cop {
    fn reg(&self, cpu: &CpuContext, idx: usize) -> u128;
    fn set_reg(&mut self, cpu: &mut CpuContext, idx: usize, val: u128);

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32, t: &Tracer) -> Result<()>;
    fn decode(&self, _opcode: u32, _pc: u64) -> DecodedInsn {
        DecodedInsn::new0("unkcop")
    }

    fn lwc(&mut self, op: u32, ctx: &mut CpuContext, bus: &Bus) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = bus.read::<u32>(ea & 0x1FFF_FFFC) as u64;
        self.set_reg(ctx, rt, val as u128);
    }

    fn ldc(&mut self, op: u32, ctx: &mut CpuContext, bus: &Bus) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = bus.read::<u64>(ea & 0x1FFF_FFFC) as u64;
        self.set_reg(ctx, rt, val as u128);
    }

    fn swc(&mut self, op: u32, ctx: &CpuContext, bus: &mut Bus) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = self.reg(ctx, rt) as u32;
        bus.write::<u32>(ea & 0x1FFF_FFFC, val);
    }

    fn sdc(&mut self, op: u32, ctx: &CpuContext, bus: &mut Bus) {
        let rt = ((op >> 16) & 0x1f) as usize;
        let ea = ctx.regs[((op >> 21) & 0x1f) as usize] as u32 + (op & 0xffff) as i16 as i32 as u32;
        let val = self.reg(ctx, rt) as u64;
        bus.write::<u64>(ea & 0x1FFF_FFFC, val);
    }

    // Implement some debugger views
    fn render_debug<'a, 'ui>(&mut self, _dr: &DebuggerRenderer<'a, 'ui>) {}

    // Internal check to efficiently handle empty coprocessors
    #[doc(hidden)]
    fn is_null() -> bool {
        false
    }
    #[doc(hidden)]
    fn is_null_obj(&self) -> bool {
        false
    }
}

/// Cop0 is a MIPS64 coprocessor #0, which (in addition to being a normal coprocessor)
/// it is able to control execution of the core by triggering exceptions.
pub trait Cop0: Cop {
    // Change a single external interrupt line.
    // Notice that hwint line 0 is mapped to bit IP2 in Cause
    // (because IP0/IP1 are used for software interrupts).
    fn set_hwint_line(&mut self, line: usize, status: bool);

    /// Poll pending interrupts. This function is called in the main interpreter
    /// loop very often, so that Cop0 has a chance of triggering interrupts
    /// when they are raised.
    /// NOTE: remember to mark this function as #[inline(always)] for maximum
    /// performance.
    fn poll_interrupts(&mut self, ctx: &mut CpuContext);

    /// Trigger the specified excepion.
    fn exception(&mut self, ctx: &mut CpuContext, exc: Exception);
}

pub struct CopNull {}

impl Cop for CopNull {
    fn reg(&self, _ctx: &CpuContext, _idx: usize) -> u128 {
        0
    }
    fn set_reg(&mut self, _ctx: &mut CpuContext, _idx: usize, _val: u128) {}

    fn op(&mut self, _cpu: &mut CpuContext, _opcode: u32, _t: &Tracer) -> Result<()> {
        Ok(())
    }
    fn decode(&self, _opcode: u32, _pc: u64) -> DecodedInsn {
        DecodedInsn::new0("unkcop")
    }
    fn is_null() -> bool {
        true
    }
    fn is_null_obj(&self) -> bool {
        true
    }
}
