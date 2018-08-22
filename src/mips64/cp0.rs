use super::cpu::{Cop, Cop0, CpuContext, Exception};
use slog;

pub struct Cp0 {
    reg_status: u64,
    reg_cause: u64,

    logger: slog::Logger,
}

impl Cp0 {
    pub fn new(logger: slog::Logger) -> Box<Cp0> {
        Box::new(Cp0 {
            reg_status: 0,
            reg_cause: 0,
            logger: logger,
        })
    }
}

impl Cop0 for Cp0 {
    fn pending_int(&self) -> bool {
        false
    }

    fn exception(&mut self, _ctx: &mut CpuContext, _exc: Exception) {}
}

struct C0op<'a> {
    opcode: u32,
    cop0: &'a mut Cp0,
    cpu: &'a mut CpuContext,
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

impl Cop for Cp0 {
    fn reg(&self, idx: usize) -> u128 {
        match idx {
            12 => self.reg_status as u128,
            13 => self.reg_cause as u128,
            _ => unimplemented!(),
        }
    }

    fn set_reg(&mut self, idx: usize, val: u128) {
        match idx {
            12 => self.reg_status = val as u64,
            13 => self.reg_cause = val as u64,
            _ => unimplemented!(),
        }
    }

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32) {
        let op = C0op {
            opcode,
            cpu,
            cop0: self,
        };
        match op.func() {
            0x00 => {
                // MFC0
                match op.rd() {
                    12 => {
                        op.cpu.regs[op.rt()] = op.cop0.reg_status;
                        op.cpu.tight_exit = true;
                    }
                    13 => {
                        op.cpu.regs[op.rt()] = op.cop0.reg_cause;
                        op.cpu.tight_exit = true;
                    }
                    _ => warn!(
                        op.cop0.logger,
                        "unimplemented COP0 read32";
                        o!("reg" => op.rd())
                    ),
                }
            }
            0x04 => {
                // MTC0 - write32
                let sel = op.sel();
                match op.rd() {
                    12 if sel == 0 => {
                        op.cop0.reg_status = op.rt64();
                        op.cpu.tight_exit = true;
                    }
                    13 if sel == 0 => {
                        op.cop0.reg_cause = op.rt64();
                        op.cpu.tight_exit = true;
                    }
                    _ => warn!(
                        op.cop0.logger,
                        "unimplemented COP0 write32";
                        o!("reg" => op.rd())
                    ),
                }
            }
            _ => panic!("unimplemented COP0 opcode: func={:x?}", op.func()),
        }
    }
}
