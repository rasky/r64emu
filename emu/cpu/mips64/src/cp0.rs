use bitfield::bitfield;

use super::decode::{DecodedInsn, REG_NAMES};
use super::{Cop, Cop0, CpuContext, Exception};
use emu::dbg::{DebuggerRenderer, Operand, RegisterSize, RegisterView, Result, Tracer};
use emu::int::Numerics;
use emu::state::Field;
use serde_derive::{Deserialize, Serialize};
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
    #[derive(Default, Copy, Clone, Serialize, Deserialize)]
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
    #[derive(Default, Copy, Clone, Serialize, Deserialize)]
    struct RegCause(u32);
    impl Debug;
    pub exc, set_exc: 6,2; // Current exception code
    pub ip, set_ip: 15,8;  // Interrupt pending (8 lines)
    pub wp, set_wp: 22;    // Watch exception
    pub iv, set_iv: 23;    // General/Special exception vector
    pub ce, set_ce: 29,28; // COP enabled exception
    pub bd, set_bd: 31;    // Exception taken from delay slot
}

#[derive(Default, Copy, Clone, Serialize, Deserialize)]
struct Cp0Context {
    reg_status: RegStatus,
    reg_cause: RegCause,
    reg_errorepc: u64,
    reg_epc: u64,
    reg_index: u32,
    reg_pagemask: u32,
    reg_entryhi: u64,
    reg_entrylo0: u64,
    reg_entrylo1: u64,
    reg_compare: u32,
    last_count: u32,
    last_count_clock: i64,
    next_timer_interrupt: i64,
}

pub struct Cp0 {
    ctx: Field<Cp0Context>,
    logger: slog::Logger,
    name: &'static str,
}

impl Cp0 {
    pub fn new(name: &'static str, logger: slog::Logger) -> Cp0 {
        Cp0 {
            ctx: Field::new(&("mips64::cp0::".to_owned() + name), Cp0Context::default()),
            logger: logger,
            name,
        }
    }

    fn get_count(&self, cpu: &CpuContext) -> u32 {
        self.ctx
            .last_count
            .wrapping_add(((cpu.clock - self.ctx.last_count_clock) >> 1) as u32)
    }

    fn set_count(&mut self, cpu: &CpuContext, val: u32) {
        info!(self.logger, "COP0 write count"; "val" => val);
        self.ctx.last_count = val;
        self.ctx.last_count_clock = cpu.clock;
        self.update_timer_interrupt(cpu);
    }

    fn set_compare(&mut self, cpu: &CpuContext, val: u32) {
        info!(self.logger, "COP0 write compare"; "val" => val);

        self.ctx.reg_compare = val;
        self.update_timer_interrupt(cpu);

        // Writing compare also clears the IRQ line (IP5 in Cause)
        self.set_hwint_line(5, false);
    }

    fn update_timer_interrupt(&mut self, cpu: &CpuContext) {
        // Compute the CPU clock at which there will be the next timer interrupt.
        // There always is a potential timer interrupt in the future because of
        // the 32-bit wrap-around.
        self.ctx.next_timer_interrupt =
            cpu.clock + ((self.ctx.reg_compare.wrapping_sub(self.get_count(cpu)) as i64) << 1);
        info!(self.logger, "COP0 update timer IRQ";
            "clock" => cpu.clock,
            "next_irq" => self.ctx.next_timer_interrupt,
            "count" => self.get_count(cpu),
            "compare" => self.ctx.reg_compare);
    }
}

impl Cop0 for Cp0 {
    #[inline(always)]
    fn set_hwint_line(&mut self, line: usize, status: bool) {
        let mut ip = self.ctx.reg_cause.ip();
        let mask = 1 << (line + 2);
        let val = (status as u32) << (line + 2);

        ip = (ip & !mask) | (val & mask);
        self.ctx.reg_cause.set_ip(ip);
    }

    #[inline(always)]
    fn poll_interrupts(&mut self, cpu: &mut CpuContext) {
        let ctx = unsafe { self.ctx.as_mut() };
        if cpu.clock >= ctx.next_timer_interrupt {
            self.set_hwint_line(5, true);
            ctx.next_timer_interrupt += 0x8000_0000; // 2**32 / 2
            info!(self.logger, "COP0 timer IRQ raised");
        }
        if ctx.reg_status.ie()
            && !ctx.reg_status.erl()
            && !ctx.reg_status.exl()
            && ctx.reg_cause.ip() & ctx.reg_status.im() != 0
        {
            self.exception(cpu, Exception::Interrupt);
        }
    }

    fn exception(&mut self, cpu: &mut CpuContext, exc: Exception) {
        use self::Exception::*;

        info!(self.logger, "exception"; "exc" => ?exc);
        let ctx = unsafe { self.ctx.as_mut() };

        match exc {
            ColdReset => {
                // ctx.reg_random = 31;
                // ctx.reg_wired = 0;
                // ctx.reg_config.set_k0(2);
                // ctx.reg_config[0..3] should be configured as specified in MipsConfig
                ctx.reg_status.set_rp(false);
                ctx.reg_status.set_bev(true);
                ctx.reg_status.set_ts(false);
                ctx.reg_status.set_sr(false);
                ctx.reg_status.set_nmi(false);
                ctx.reg_status.set_erl(true);
                // self.watch_lo[..] = 0;
                // ctx.reg_perfcnt[..].set_ie(0);
                ctx.reg_epc = cpu.pc;
                cpu.set_pc(0xFFFF_FFFF_BFC0_0000);
            }
            SoftReset => {
                // self.ref_config.set_k0(2);
                ctx.reg_status.set_rp(false);
                ctx.reg_status.set_bev(true);
                ctx.reg_status.set_ts(false);
                ctx.reg_status.set_sr(true);
                ctx.reg_status.set_nmi(false);
                ctx.reg_status.set_erl(true);
                // self.watch_lo[..] = 0;
                // ctx.reg_perfcnt[..].set_ie(0);
                ctx.reg_epc = cpu.pc;
                cpu.set_pc(0xFFFF_FFFF_BFC0_0000);
            }
            Nmi => {
                error!(self.logger, "unimplemented exception type"; "exc" => ?exc);
            }
            _ => {
                // Standard exception
                let vector = if !ctx.reg_status.exl() {
                    if !cpu.delay_slot {
                        ctx.reg_epc = cpu.pc;
                        ctx.reg_cause.set_bd(false);
                    } else {
                        ctx.reg_epc = cpu.pc - 4;
                        ctx.reg_cause.set_bd(true);
                    }

                    match exc {
                        TlbRefill => 0x0,
                        XTlbRefill => 0x80,
                        Interrupt if ctx.reg_cause.iv() => 0x200,
                        _ => 0x180,
                    }
                } else {
                    0x180
                };

                // Coprocessor unit number
                ctx.reg_cause.set_ce(0);
                ctx.reg_cause.set_exc(exc.exc_code().unwrap_or(0));
                ctx.reg_status.set_exl(true);
                if ctx.reg_status.bev() {
                    cpu.set_pc(0xFFFF_FFFF_BFC0_0200 + vector);
                } else {
                    cpu.set_pc(0xFFFF_FFFF_8000_0000 + vector);
                }
            }
        };
    }
}

impl Cop for Cp0 {
    fn reg(&self, cpu: &CpuContext, idx: usize) -> u128 {
        match idx {
            0 => self.ctx.reg_index as u128,
            2 => self.ctx.reg_entrylo0 as u128,
            3 => self.ctx.reg_entrylo1 as u128,
            5 => self.ctx.reg_pagemask as u128,
            9 => self.get_count(cpu) as u128,
            10 => self.ctx.reg_entryhi as u128,
            11 => self.ctx.reg_compare as u128,
            12 => self.ctx.reg_status.0 as u128,
            13 => self.ctx.reg_cause.0 as u128,
            14 => self.ctx.reg_epc as u128,
            30 => self.ctx.reg_errorepc as u128,
            _ => {
                error!(
                    self.logger,
                    "unimplemented COP0 reg read";
                    "reg" => COP0_REG_NAMES[idx],
                );
                0
            }
        }
    }

    fn set_reg(&mut self, cpu: &mut CpuContext, idx: usize, val: u128) {
        match idx {
            0 => self.ctx.reg_index = val as u32 & 0x1F,
            2 => self.ctx.reg_entrylo0 = val as u64,
            3 => self.ctx.reg_entrylo1 = val as u64,
            5 => self.ctx.reg_pagemask = val as u32,
            9 => self.set_count(cpu, val as u32),
            10 => self.ctx.reg_entryhi = val as u64,
            11 => self.set_compare(cpu, val as u32),
            12 => {
                self.ctx.reg_status.0 = val as u32;
                cpu.tight_exit = true;
            }
            13 => {
                self.ctx.reg_cause.0 = val as u32;
                cpu.tight_exit = true;
            }
            14 => self.ctx.reg_epc = val as u64,
            30 => self.ctx.reg_errorepc = val as u64,
            _ => {
                error!(
                    self.logger,
                    "unimplemented COP0 reg write";
                    "reg" => COP0_REG_NAMES[idx], "val" => val,
                );
            }
        }
    }

    fn op(&mut self, cpu: &mut CpuContext, opcode: u32, t: &Tracer) -> Result<()> {
        let func = opcode & 0x3F;
        let rs = ((opcode >> 21) & 0x1f) as usize;
        let rt = ((opcode >> 16) & 0x1f) as usize;
        let rd = ((opcode >> 11) & 0x1f) as usize;
        let ctx = unsafe { self.ctx.as_mut() };

        match rs {
            0x00 => {
                // MFC0
                let _sel = opcode & 7;
                cpu.regs[rt] = self.reg(cpu, rd) as u64;
            }
            0x04 => {
                // MTC0
                let _sel = opcode & 7;
                self.set_reg(cpu, rd, cpu.regs[rt] as u128);
            }
            0x10..=0x1F => match func {
                0x01 => {
                    // TLBR
                    let entry = cpu.mmu.read(ctx.reg_index as usize);
                    ctx.reg_entryhi = entry.hi();
                    ctx.reg_entrylo0 = entry.lo0;
                    ctx.reg_entrylo1 = entry.lo1;
                    ctx.reg_pagemask = entry.page_mask;
                    info!(self.logger, "read TLB entry";
                        "idx" => ctx.reg_index,
                        "tlb" => ?entry);
                }
                0x02 => {
                    // TLBWI
                    cpu.mmu.write(
                        ctx.reg_index as usize,
                        ctx.reg_pagemask,
                        ctx.reg_entryhi,
                        ctx.reg_entrylo0,
                        ctx.reg_entrylo1,
                    );

                    info!(self.logger, "wrote TLB entry";
                        "idx" => ctx.reg_index,
                        "tlb" => ?cpu.mmu.read(ctx.reg_index as usize));
                }
                0x08 => {
                    // TLBP
                    match cpu.mmu.probe(ctx.reg_entryhi, ctx.reg_entryhi as u8) {
                        Some(idx) => {
                            info!(self.logger, "probe TLB entry: found";
                                "entry_hi" => ctx.reg_entryhi.hex(),
                                "found_idx" => idx,
                                "found_tlb" => ?cpu.mmu.read(ctx.reg_index as usize));
                            ctx.reg_index = idx as u32;
                        }
                        None => {
                            info!(self.logger, "probe TLB entry: not found";
                                "entry_hi" => ctx.reg_entryhi.hex());
                            ctx.reg_index = 0x8000_0000;
                        }
                    };
                }
                0x18 => {
                    // ERET
                    // FIXME: verify that it's a NOP when ERL/EXL are 0
                    if ctx.reg_status.erl() {
                        ctx.reg_status.set_erl(false);
                        cpu.set_pc(ctx.reg_errorepc);
                    } else if ctx.reg_status.exl() {
                        ctx.reg_status.set_exl(false);
                        cpu.set_pc(ctx.reg_epc);
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
        let ctx = unsafe { self.ctx.as_mut() };

        match col {
            0 => {
                let status = format!(
                    "IM:{:08b} IE:{} EXL:{} ERL:{}",
                    ctx.reg_status.im(),
                    ctx.reg_status.ie() as u8,
                    ctx.reg_status.exl() as u8,
                    ctx.reg_status.erl() as u8,
                );
                let cause = format!(
                    "IP:{:08b} EXC:{} BD:{}",
                    ctx.reg_cause.ip(),
                    ctx.reg_cause.exc(),
                    ctx.reg_cause.bd() as u8,
                );
                visit("Status", Reg32(&mut ctx.reg_status.0), Some(&status));
                visit("Cause", Reg32(&mut ctx.reg_cause.0), Some(&cause));
                visit("EPC", Reg64(&mut ctx.reg_epc), None);
                visit("ErrorEPC", Reg64(&mut ctx.reg_errorepc), None);

                visit("Index", Reg32(&mut ctx.reg_index), None);
                visit("PageMask", Reg32(&mut ctx.reg_pagemask), None);
                visit("EntryHi", Reg64(&mut ctx.reg_entryhi), None);
                visit("EntryLo0", Reg64(&mut ctx.reg_entrylo0), None);
                visit("EntryLo1", Reg64(&mut ctx.reg_entrylo1), None);

                visit("Compare", Reg32(&mut ctx.reg_compare), None);
            }
            _ => unreachable!(),
        }
    }
}
