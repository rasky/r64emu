use bitfield::bitfield;

use super::cpu::{Cop, Cop0, CpuContext, Exception};
use super::decode::{DecodedInsn, REG_NAMES};
use emu::dbg::{Operand, Result, Tracer};
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

bitfield! {
    struct RegStatus(u64);
    impl Debug;
    pub ie, set_ie2: 0;
    pub exl, set_exl: 1;
    pub erl, set_erl: 2;
}

pub struct Cp0 {
    reg_status: RegStatus,
    reg_cause: u64,
    reg_errorepc: u64,
    reg_epc: u64,

    logger: slog::Logger,
}

impl Cp0 {
    pub fn new(logger: slog::Logger) -> Box<Cp0> {
        Box::new(Cp0 {
            reg_status: RegStatus(0),
            reg_cause: 0,
            reg_epc: 0,
            reg_errorepc: 0,
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

impl Cop for Cp0 {
    fn reg(&self, idx: usize) -> u128 {
        match idx {
            12 => self.reg_status.0 as u128,
            13 => self.reg_cause as u128,
            14 => self.reg_epc as u128,
            30 => self.reg_errorepc as u128,
            _ => 0,
        }
    }

    fn set_reg(&mut self, idx: usize, val: u128) {
        match idx {
            12 => self.reg_status.0 = val as u64,
            13 => self.reg_cause = val as u64,
            14 => self.reg_epc = val as u64,
            30 => self.reg_errorepc = val as u64,
            _ => {}
        }
    }

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32, t: &Tracer) -> Result<()> {
        let func = opcode & 0x3F;
        let rs = ((opcode >> 21) & 0x1f) as usize;
        let rt = ((opcode >> 16) & 0x1f) as usize;
        let rd = ((opcode >> 11) & 0x1f) as usize;

        match rs {
            0x00 => {
                // MFC0
                let _sel = opcode & 7;
                cpu.regs[rt] = self.reg(rd) as u64;
                match rd {
                    12 | 13 | 14 | 30 => {}
                    _ => warn!(
                        self.logger,
                        "unimplemented COP0 read32";
                        "reg" => COP0_REG_NAMES[rd]
                    ),
                }
            }
            0x04 => {
                // MTC0 - write32
                let _sel = opcode & 7;
                self.set_reg(rd, cpu.regs[rt] as u128);
                match rd {
                    // Status/Cause: exception status might change
                    12 | 13 => cpu.tight_exit = true,
                    14 | 30 => {}
                    _ => warn!(
                        self.logger,
                        "unimplemented COP0 write32";
                        "reg" => COP0_REG_NAMES[rd], "val" => cpu.regs[rt].hex(),
                    ),
                }
            }
            0x10..=0x1F => match func {
                0x18 => {
                    // ERET
                    // FIXME: verify that it's a NOP when ERL/EXL are 0
                    if self.reg_status.erl() {
                        self.reg_status.set_erl(false);
                        cpu.set_pc(self.reg_errorepc);
                    } else if self.reg_status.exl() {
                        self.reg_status.set_exl(false);
                        cpu.set_pc(self.reg_epc);
                    }
                }
                _ => {
                    error!(self.logger, "unimplemented COP0 opcode"; "func" => func.hex());
                    return t.break_here("unimplemented COP0 opcode");
                }
            },
            _ => {
                error!(self.logger, "unimplemented COP0 function"; "rs" => rs);
                return t.break_here("unimplemented COP0 function");
            }
        };
        Ok(())
    }

    fn decode(&self, opcode: u32, _pc: u64) -> DecodedInsn {
        use self::Operand::*;

        let func = opcode & 0x3f;
        let rs = (opcode >> 21) & 0x1f;
        let vrt = (opcode >> 16) as usize & 0x1f;
        let vrd = (opcode >> 11) as usize & 0x1f;
        let rt = REG_NAMES[vrt];
        let _rd = REG_NAMES[vrd];
        let _c0rt = COP0_REG_NAMES[vrt];
        let c0rd = COP0_REG_NAMES[vrd];

        match rs {
            0x00 => DecodedInsn::new2("mfc0", OReg(rt), IReg(c0rd)),
            0x04 => DecodedInsn::new2("mtc0", IReg(rt), OReg(c0rd)),
            0x10..=0x1F => match func {
                0x1 => DecodedInsn::new0("tlbr"),
                0x2 => DecodedInsn::new0("tlbwi"),
                0x8 => DecodedInsn::new0("tlbp"),
                0x18 => DecodedInsn::new0("eret"),
                _ => DecodedInsn::new1("cop0op?", Imm32(func)),
            },
            _ => DecodedInsn::new1("cop0?", Imm32(rs)),
        }
    }
}
