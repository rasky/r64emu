use super::{Sp, StatusFlags};
use emu::bus::be::{Bus, DevPtr, Device};
use emu::int::Numerics;
use errors::*;
use mips64;

pub struct SpCop0 {
    sp: DevPtr<Sp>,
    reg_bus: Box<Bus>, // bus to access SP HW registers via MTC/MFC
    logger: slog::Logger,
}

impl SpCop0 {
    pub fn new(sp: &DevPtr<Sp>, logger: slog::Logger) -> Result<Box<SpCop0>> {
        // Bank #1 in sp are the SP HW registers. Map them into a local
        // bus that we can use to access them in MTC/MFC.
        let mut reg_bus = Bus::new(logger.new(o!()));
        sp.borrow().dev_map(&mut reg_bus, 1, 0x0000_0000)?;

        Ok(Box::new(SpCop0 {
            logger: logger,
            sp: sp.clone(),
            reg_bus: reg_bus,
        }))
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
    fn sel(&self) -> u32 {
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
                let sp = self.sp.borrow_mut();
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

    fn op(&mut self, cpu: &mut mips64::CpuContext, opcode: u32) {
        let op = C0op {
            opcode,
            cpu,
            cop0: self,
        };
        match op.func() {
            0x04 => {
                // MTC0: write to SP HW register
                let rd = op.rd() as u32;
                info!(op.cop0.logger, "RSP MTC0"; "reg" => rd.hex(), "val" => op.rt32().hex());
                op.cop0.reg_bus.write::<u32>(rd * 4, op.rt32());
            }
            _ => panic!("unimplemented RSP COP0 opcode: func={:x?}", op.func()),
        }
    }
}
