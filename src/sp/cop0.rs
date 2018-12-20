use super::super::dp::Dp;
use super::{Sp, StatusFlags};
use crate::errors::*;
use emu::bus::be::{Bus, DevPtr, Device};
use emu::dbg;
use emu::dbg::{Operand, Tracer};
use mips64;
use mips64::{DecodedInsn, REG_NAMES};

const RSP_COP0_REG_NAMES: [&'static str; 32] = [
    "DMA_CACHE",
    "DMA_DRAM",
    "DMA_READ_LENGTH",
    "DMA_WRITE_LENTGH",
    "SP_STATUS",
    "DMA_FULL",
    "DMA_BUSY",
    "SP_SEMAPHORE",
    "CMD_START",
    "CMD_END",
    "CMD_CURRENT",
    "CMD_STATUS",
    "CMD_CLOCK",
    "CMD_BUSY",
    "CMD_PIPE_BUSY",
    "CMD_TMEM_BUSY",
    "?16?",
    "?17?",
    "?18?",
    "?19?",
    "?20?",
    "?21?",
    "?22?",
    "?23?",
    "?24?",
    "?25?",
    "?26?",
    "?27?",
    "?28?",
    "?29?",
    "?30?",
    "?31?",
];

pub struct SpCop0 {
    sp: DevPtr<Sp>,
    reg_bus: Box<Bus>, // bus to access SP HW registers via MTC/MFC
    _logger: slog::Logger,
}

impl SpCop0 {
    pub fn new(sp: &DevPtr<Sp>, dp: &DevPtr<Dp>, logger: slog::Logger) -> Result<SpCop0> {
        // Bank #1 in sp are the SP HW registers. Map them into a local
        // bus that we can use to access them in MTC/MFC.
        // Same for DP registers.
        let mut reg_bus = Bus::new(logger.new(o!()));
        sp.borrow().dev_map(&mut reg_bus, 1, 0x0000_0000)?;
        dp.borrow().dev_map(&mut reg_bus, 0, 0x0000_0020)?;

        Ok(SpCop0 {
            _logger: logger,
            sp: sp.clone(),
            reg_bus: reg_bus,
        })
    }
}

struct C0op<'a> {
    opcode: u32,
    cop0: &'a mut SpCop0,
    cpu: &'a mut mips64::CpuContext,
}

impl<'a> C0op<'a> {
    fn func(&self) -> usize {
        ((self.opcode >> 21) & 0x1f) as usize
    }
    fn _sel(&self) -> u32 {
        self.opcode & 7
    }
    fn rt(&self) -> usize {
        ((self.opcode >> 16) & 0x1f) as usize
    }
    fn rd(&self) -> usize {
        ((self.opcode >> 11) & 0x1f) as usize
    }
    fn rt64(&self) -> u64 {
        self.cpu.regs[self.rt()]
    }
    fn rt32(&self) -> u32 {
        self.rt64() as u32
    }
    fn mrt64(&'a mut self) -> &'a mut u64 {
        &mut self.cpu.regs[self.rt()]
    }
}

impl mips64::Cop0 for SpCop0 {
    fn set_hwint_line(&mut self, line: usize, status: bool) {
        return; // RSP has no interrupts
    }

    fn pending_int(&self) -> bool {
        false // RSP has no interrupts
    }

    fn exception(&mut self, ctx: &mut mips64::CpuContext, exc: mips64::Exception) {
        match exc {
            mips64::Exception::ColdReset => {
                ctx.set_halt_line(true);
                ctx.set_pc(0);
            }

            // Breakpoint exception is used by RSP to halt itself
            mips64::Exception::Breakpoint => {
                let mut sp = self.sp.borrow_mut();
                let mut status = sp.get_status();
                status.insert(StatusFlags::HALT | StatusFlags::BROKE);
                match sp.set_status(status) {
                    Some(halt) => ctx.set_halt_line(halt),
                    None => {}
                }
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

    fn op(&mut self, cpu: &mut mips64::CpuContext, opcode: u32, t: &Tracer) -> dbg::Result<()> {
        let mut op = C0op {
            opcode,
            cpu,
            cop0: self,
        };
        match op.func() {
            0x00 => {
                // MFC0: read from SP HW register
                let rd = op.rd() as u32;
                *op.mrt64() = op.cop0.reg_bus.read::<u32>(rd * 4) as u64;
            }
            0x04 => {
                // MTC0: write to SP HW register
                let reg = op.rd() as u32 * 4;
                let val = op.rt32();

                // HACK: writing the status register can trigger the HALT flag.
                // Rust blocks these kind of circular references. Workaround
                // by peeling some layer
                if reg == 0x10 {
                    let mut sp = op.cop0.sp.borrow_mut();
                    match sp.write_status(val) {
                        Some(halt) => op.cpu.set_halt_line(halt),
                        None => {}
                    }
                    return Ok(());
                }

                op.cop0.reg_bus.write::<u32>(reg, val);
            }
            _ => panic!("unimplemented RSP COP0 opcode: func={:x?}", op.func()),
        }
        Ok(())
    }

    fn decode(&self, opcode: u32, _pc: u64) -> DecodedInsn {
        use self::Operand::*;

        let func = (opcode >> 21) & 0x1f;
        let vrt = (opcode >> 16) as usize & 0x1f;
        let vrd = (opcode >> 11) as usize & 0x1f;
        let rt = REG_NAMES[vrt];
        let _rd = REG_NAMES[vrd];
        let _c0rt = RSP_COP0_REG_NAMES[vrt];
        let c0rd = RSP_COP0_REG_NAMES[vrd];

        match func {
            0x00 => DecodedInsn::new2("mfc0", OReg(rt), IReg(c0rd)),
            0x04 => DecodedInsn::new2("mtc0", IReg(rt), OReg(c0rd)),
            _ => DecodedInsn::new1("cop0", Imm32(func)),
        }
    }
}
