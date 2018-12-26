use bitfield::bitfield;

use super::decode::{DecodedInsn, REG_NAMES};
use super::{Cop, Cop0, CpuContext, Exception};
use emu::dbg::{DebuggerRenderer, Operand, RegisterSize, RegisterView, Result, Tracer};
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
    struct RegStatus(u32);
    impl Debug;
    pub ie, set_ie: 0;    // Interrupt enable
    pub exl, set_exl: 1;  // Is within standard exception
    pub erl, set_erl: 2;  // Is within special exception (reset/nmi)
    pub im, set_im: 15,8; // Interrupt mask (8 lines)
    pub nmi, set_nmi: 19; // Are we under NMI?
    pub sr, set_sr: 20;   // Is this a soft reset?
    pub ts, set_ts: 21;   // Multiple TLB match
    pub bev, set_bev: 22; // Exception vector location (normal/bootstrap)
    pub rp, set_rp: 21;   // Reduced power
    pub cu0, set_cu0: 28; // Is COP0 active?
    pub cu1, set_cu1: 29; // Is COP1 active?
    pub cu2, set_cu2: 30; // Is COP2 active?
    pub cu3, set_cu3: 31; // Is COP3 active?
}

bitfield! {
    struct RegCause(u32);
    impl Debug;
    pub exc, set_exc: 6,2; // Current exception code
    pub ip, set_ip: 15,8;  // Interrupt pending (8 lines)
    pub wp, set_wp: 22;    // Watch exception
    pub iv, set_iv: 23;    // General/Special exception vector
    pub ce, set_ce: 29,28; // COP enabled exception
    pub bd, set_bd: 31;    // Exception taken from delay slot
}

pub struct Cp0 {
    reg_status: RegStatus,
    reg_cause: RegCause,
    reg_errorepc: u64,
    reg_epc: u64,
    reg_index: u32,
    reg_pagemask: u32,
    reg_entryhi: u64,
    reg_entrylo0: u64,
    reg_entrylo1: u64,

    logger: slog::Logger,
    name: &'static str,
}

impl Cp0 {
    pub fn new(name: &'static str, logger: slog::Logger) -> Cp0 {
        Cp0 {
            reg_status: RegStatus(0),
            reg_cause: RegCause(0),
            reg_epc: 0,
            reg_errorepc: 0,
            reg_index: 0,
            reg_pagemask: 0,
            reg_entrylo0: 0,
            reg_entrylo1: 0,
            reg_entryhi: 0,
            logger: logger,
            name,
        }
    }
}

impl Cop0 for Cp0 {
    fn set_hwint_line(&mut self, line: usize, status: bool) {
        let mut ip = self.reg_cause.ip();
        let mask = 1 << (line + 2);
        let val = (status as u32) << (line + 2);

        ip = (ip & !mask) | (val & mask);
        self.reg_cause.set_ip(ip);
    }

    fn pending_int(&self) -> bool {
        self.reg_status.ie()
            && !self.reg_status.erl()
            && !self.reg_status.exl()
            && self.reg_cause.ip() & self.reg_status.im() != 0
    }

    fn exception(&mut self, cpu: &mut CpuContext, exc: Exception) {
        use self::Exception::*;

        info!(self.logger, "exception"; "exc" => ?exc);

        match exc {
            ColdReset => {
                // self.reg_random = 31;
                // self.reg_wired = 0;
                // self.reg_config.set_k0(2);
                // self.reg_config[0..3] should be configured as specified in MipsConfig
                self.reg_status.set_rp(false);
                self.reg_status.set_bev(true);
                self.reg_status.set_ts(false);
                self.reg_status.set_sr(false);
                self.reg_status.set_nmi(false);
                self.reg_status.set_erl(true);
                // self.watch_lo[..] = 0;
                // self.reg_perfcnt[..].set_ie(0);
                self.reg_epc = cpu.pc;
                cpu.set_pc(0xFFFF_FFFF_BFC0_0000);
            }
            SoftReset => {
                // self.ref_config.set_k0(2);
                self.reg_status.set_rp(false);
                self.reg_status.set_bev(true);
                self.reg_status.set_ts(false);
                self.reg_status.set_sr(true);
                self.reg_status.set_nmi(false);
                self.reg_status.set_erl(true);
                // self.watch_lo[..] = 0;
                // self.reg_perfcnt[..].set_ie(0);
                self.reg_epc = cpu.pc;
                cpu.set_pc(0xFFFF_FFFF_BFC0_0000);
            }
            Nmi => {
                error!(self.logger, "unimplemented exception type"; "exc" => ?exc);
            }
            _ => {
                // Standard exception
                let vector = if !self.reg_status.exl() {
                    if !cpu.delay_slot {
                        self.reg_epc = cpu.pc;
                        self.reg_cause.set_bd(false);
                    } else {
                        self.reg_epc = cpu.pc - 4;
                        self.reg_cause.set_bd(true);
                    }

                    match exc {
                        TlbRefill => 0x0,
                        XTlbRefill => 0x80,
                        Interrupt if self.reg_cause.iv() => 0x200,
                        _ => 0x180,
                    }
                } else {
                    0x180
                };

                // Coprocessor unit number
                self.reg_cause.set_ce(0);
                self.reg_cause.set_exc(exc.exc_code().unwrap_or(0));
                self.reg_status.set_exl(true);
                if self.reg_status.bev() {
                    cpu.set_pc(0xFFFF_FFFF_BFC0_0200 + vector);
                } else {
                    cpu.set_pc(0xFFFF_FFFF_8000_0000 + vector);
                }
            }
        };
    }
}

impl Cop for Cp0 {
    fn reg(&self, idx: usize) -> u128 {
        match idx {
            0 => self.reg_index as u128,
            2 => self.reg_entrylo0 as u128,
            3 => self.reg_entrylo1 as u128,
            5 => self.reg_pagemask as u128,
            10 => self.reg_entryhi as u128,
            12 => self.reg_status.0 as u128,
            13 => self.reg_cause.0 as u128,
            14 => self.reg_epc as u128,
            30 => self.reg_errorepc as u128,
            _ => 0,
        }
    }

    fn set_reg(&mut self, idx: usize, val: u128) {
        match idx {
            0 => self.reg_index = val as u32 & 0x1F,
            2 => self.reg_entrylo0 = val as u64,
            3 => self.reg_entrylo1 = val as u64,
            5 => self.reg_pagemask = val as u32,
            10 => self.reg_entryhi = val as u64,
            12 => self.reg_status.0 = val as u32,
            13 => self.reg_cause.0 = val as u32,
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
                    0 | 2 | 3 | 5 | 10 => {}                                   // TLB regs
                    9 => cpu.regs[rt] = (cpu.clock as u64 >> 1) & 0xFFFF_FFFF, // Count
                    12 | 13 => {}                                              // Status / Cause
                    14 | 30 => {}                                              // EPC / ErrorEPC
                    _ => warn!(
                        self.logger,
                        "unimplemented COP0 read32";
                        "reg" => COP0_REG_NAMES[rd],
                        "val" => cpu.regs[rt].hex(),
                    ),
                }
            }
            0x04 => {
                // MTC0 - write32
                let _sel = opcode & 7;
                self.set_reg(rd, cpu.regs[rt] as u128);
                match rd {
                    0 | 2 | 3 | 5 | 10 => {}          // TLB regs
                    12 | 13 => cpu.tight_exit = true, // Status / Cause
                    14 | 30 => {}                     // EPC / ErrorEPC
                    _ => warn!(
                        self.logger,
                        "unimplemented COP0 write32";
                        "reg" => COP0_REG_NAMES[rd], "val" => cpu.regs[rt].hex(),
                    ),
                }
            }
            0x10..=0x1F => match func {
                0x01 => {
                    // TLBR
                    let entry = cpu.mmu.read(self.reg_index as usize);
                    self.reg_entryhi = entry.hi();
                    self.reg_entrylo0 = entry.lo0;
                    self.reg_entrylo1 = entry.lo1;
                    self.reg_pagemask = entry.page_mask;
                    info!(self.logger, "read TLB entry";
                        "idx" => self.reg_index,
                        "tlb" => ?entry);
                }
                0x02 => {
                    // TLBWI
                    cpu.mmu.write(
                        self.reg_index as usize,
                        self.reg_pagemask,
                        self.reg_entryhi,
                        self.reg_entrylo0,
                        self.reg_entrylo1,
                    );

                    info!(self.logger, "wrote TLB entry";
                        "idx" => self.reg_index,
                        "tlb" => ?cpu.mmu.read(self.reg_index as usize));
                }
                0x08 => {
                    // TLBP
                    match cpu.mmu.probe(self.reg_entryhi, self.reg_entryhi as u8) {
                        Some(idx) => {
                            info!(self.logger, "probe TLB entry: found";
                                "entry_hi" => self.reg_entryhi.hex(),
                                "found_idx" => idx,
                                "found_tlb" => ?cpu.mmu.read(self.reg_index as usize));
                            self.reg_index = idx as u32;
                        }
                        None => {
                            info!(self.logger, "probe TLB entry: not found";
                                "entry_hi" => self.reg_entryhi.hex());
                            self.reg_index = 0x8000_0000;
                        }
                    };
                }
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

    fn render_debug(&mut self, dr: &DebuggerRenderer) {
        dr.render_regview(self);
    }
}

impl RegisterView for Cp0 {
    const WINDOW_SIZE: (f32, f32) = (180.0, 400.0);
    const COLUMNS: usize = 1;

    fn name(&self) -> &str {
        self.name
    }

    fn visit_regs<'s, F>(&'s mut self, col: usize, mut visit: F)
    where
        F: for<'a> FnMut(&'a str, RegisterSize<'a>, Option<&str>),
    {
        use self::RegisterSize::*;

        match col {
            0 => {
                let status = format!(
                    "IM:{:08b} IE:{} EXL:{} ERL:{}",
                    self.reg_status.im(),
                    self.reg_status.ie() as u8,
                    self.reg_status.exl() as u8,
                    self.reg_status.erl() as u8,
                );
                let cause = format!(
                    "IP:{:08b} EXC:{} BD:{}",
                    self.reg_cause.ip(),
                    self.reg_cause.exc(),
                    self.reg_cause.bd() as u8,
                );
                visit("Status", Reg32(&mut self.reg_status.0), Some(&status));
                visit("Cause", Reg32(&mut self.reg_cause.0), Some(&cause));
                visit("EPC", Reg64(&mut self.reg_epc), None);
                visit("ErrorEPC", Reg64(&mut self.reg_errorepc), None);

                visit("Index", Reg32(&mut self.reg_index), None);
                visit("PageMask", Reg32(&mut self.reg_pagemask), None);
                visit("EntryHi", Reg64(&mut self.reg_entryhi), None);
                visit("EntryLo0", Reg64(&mut self.reg_entrylo0), None);
                visit("EntryLo1", Reg64(&mut self.reg_entrylo1), None);
            }
            _ => unreachable!(),
        }
    }
}
