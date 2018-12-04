use super::cpu::{Cop, Cop0, CpuContext, Exception};
use super::decode::{DecodedInsn, REG_NAMES};
use emu::dbg::Operand;
use emu::int::Numerics;
use slog;

const COP0_REG_NAMES: [&'static str; 32] = [
    "Index",
    "Random",
    "EntryLo0",
    "EntryLo1",
    "Context",
    "PageMask",
    "Wired",
    "?7?",
    "BadVAddr",
    "Count",
    "EntryHi",
    "Compare",
    "Status",
    "Cause",
    "EPC",
    "PRId",
    "Config",
    "LLAddr",
    "WatchLo",
    "WatchHi",
    "XContext",
    "?21?",
    "?22?",
    "?23?",
    "?24?",
    "?25?",
    "ParityError",
    "CacheError",
    "TagLo",
    "TagHi",
    "ErrorEPC",
    "?31?",
];

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
                    }
                    13 => {
                        op.cpu.regs[op.rt()] = op.cop0.reg_cause;
                    }
                    _ => warn!(
                        op.cop0.logger,
                        "unimplemented COP0 read32";
                        "reg" => op.rd()
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
                        "reg" => op.rd(), "val" => op.rt64().hex(),
                    ),
                }
            }
            _ => panic!("unimplemented COP0 opcode: func={:x?}", op.func()),
        }
    }

    fn decode(&self, opcode: u32, _pc: u64) -> DecodedInsn {
        use self::Operand::*;

        let func = (opcode >> 21) & 0x1f;
        let vrt = (opcode >> 16) as usize & 0x1f;
        let vrd = (opcode >> 11) as usize & 0x1f;
        let rt = REG_NAMES[vrt];
        let _rd = REG_NAMES[vrd];
        let _c0rt = COP0_REG_NAMES[vrt];
        let c0rd = COP0_REG_NAMES[vrd];

        match func {
            0x00 => DecodedInsn::new2("mfc0", OReg(rt), IReg(c0rd)),
            0x04 => DecodedInsn::new2("mtc0", IReg(rt), OReg(c0rd)),
            _ => DecodedInsn::new1("cop0", Imm32(func)),
        }
    }
}
