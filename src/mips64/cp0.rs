use super::cpu::{Cop, Cop0, CpuContext, Exception};
use slog;

const CP0_REG_INDEX: usize = 0;
const CP0_REG_RANDOM: usize = 1;
const CP0_REG_ENTRY_LO0: usize = 2;
const CP0_REG_ENTRY_LO1: usize = 3;
const CP0_REG_CONTEXT: usize = 4;
const CP0_REG_PAGE_MASK: usize = 5;
const CP0_REG_WIRED: usize = 6;
const CP0_REG_BAD_VADDR: usize = 8;
const CP0_REG_COUNT: usize = 9;
const CP0_REG_ENTRY_HI: usize = 10;
const CP0_REG_COMPARE: usize = 11;
const CP0_REG_STATUS: usize = 12;
const CP0_REG_CAUSE: usize = 13;
const CP0_REG_EPC: usize = 14;
const CP0_REG_PRID: usize = 15;
const CP0_REG_CONFIG: usize = 16;
const CP0_REG_LL_ADDR: usize = 17;
const CP0_REG_WATCH_LO: usize = 18;
const CP0_REG_WATCH_HI: usize = 19;
const CP0_REG_X_CONTEXT: usize = 20;
const CP0_REG_PARITY_ERROR: usize = 26;
const CP0_REG_CACHE_ERROR: usize = 27;
const CP0_REG_TAG_LO: usize = 28;
const CP0_REG_TAG_HI: usize = 29;
const CP0_REG_ERROR_EPC: usize = 30;

bitfield! {
    #[derive(Default)]
    pub struct StatusReg(u32);
    impl Debug;
    // Specifies and indicates global interrupt enable
    // (0 - disable interrupts, 1 - enable interrupts)
    ie, set_ie: 0;
    // Specifies and indicates exception level
    // (0 - normal, 1 - exception)
    exl, set_exl: 1;
    // Specifies and indicates error level
    // (0 - normal, 1 - error)
    erl, set_erl: 2;

    // Specifies and indicates mode bits
    // (10 - User, 01 - Supervisor, 00 - Kernel)
    ksu, set_ksu: 4, 3;

    // Enables 64-bit addressing and operations in User mode. When this bit is set, XTLB
    // miss exception is generated on TLB misses in User mode addresses space.
    // (0 - 32-bit, 1 - 64-bit)
    ux, set_ux: 5;

    // Enables 64-bit addressing and operations in Supervisor mode. When this bit is set, XTLB
    // miss exception is generated on TLB misses in Supervisor mode addresses space.
    // (0 - 32-bit, 1 - 64-bit)
    sx, set_sx: 6;

    // Enables 64-bit addressing in Kernel mode. When this bit is set, XTLB
    // miss exception is generated on TLB misses in Kernel mode addresses space.
    // (0 - 32-bit, 1 - 64-bit)
    // 64-bit operation is always valid in Kernel mode.
    kx, set_kx: 7;

    // Interrupt Mask field, enables external, internal, coprocessors or software interrupts.
    im, set_im: 15, 8;

    // compat bit
    ds_de, _: 16;

    // compat bit
    ds_ce, _: 17;

    // CP0 condition bit
    ds_ch, set_ds_ch: 18;

    // 0 - Indicates a Soft Reset or NMI has not occurred.
    // 1 - Indicates a Soft Reset or NMI has occurred.
    ds_sr, set_ds_sr: 20;

    // Indicates TLB shutdown has occurred (read-only)
    ds_ts, _: 21;

    // Controls the location of TLB miss and general purpose exception vectors.
    // 0 - normal
    // 1 - bootstrap
    ds_bev, set_ds_bev: 22;

    // Enables Instruction Trace Support.
    ds_its, set_ds_its: 24;

    // Reverse-Endian bit, enables reverse of system endianness in User mode.
    // (0 - disabled, 1 - reversed)
    re, set_re: 25;

    // Enables additional floating-point registers
    // (0 - 16 registers, 1 - 32 registers)
    fr, set_fr: 26;

    // Enables low-power operation by reducing the internal clock frequency and the
    // system interface clock frequency to one-quarter speed.
    // (0 - normal, 1 - low power mode)
    rp, set_rp: 27;

    // Controls the usability of each of the four coprocessor unit numbers.
    // (1 - usable, 0 - unusable)
    // CP0 is always usable when in Kernel mode, regardless of the setting of the
    // CU0 bit. CP2 and CP3 are reserved for future expansion.
    cu0, set_cu1: 28;
    cu1, set_cu0: 29;
    cu2, set_cu2: 30;
    cu3, set_cu3: 31;
}

/// System Control Coprocessor (CP0)
pub struct Cp0 {
    // Reg #0
    reg_index: u64,
    // Reg #1
    reg_random: u64,
    // Reg #2
    reg_entry_lo0: u64,
    // Reg #3
    reg_entry_lo1: u64,
    // Reg #4
    reg_context: u64,
    // Reg #5
    reg_page_mask: u64,
    // Reg #6
    reg_wired: u64,

    // Reg #8
    reg_bad_vaddr: u64,
    // Reg #9
    reg_count: u64,
    // Reg #10
    reg_entry_hi: u64,
    // Reg #11
    reg_compare: u64,
    // Reg #12
    reg_status: StatusReg,
    // Reg #13
    reg_cause: u64,
    // Reg #14
    reg_epc: u64,
    // Reg #15
    reg_pr_id: u64,
    // Reg #16
    reg_config: u64,
    // Reg #17
    reg_ll_addr: u64,
    // Reg #18
    reg_watch_lo: u64,
    // Reg #19
    reg_watch_hi: u64,
    // Reg #20
    reg_x_context: u64,

    // Reg #26
    reg_parity_error: u64,
    // Reg #27
    reg_cache_error: u64,
    // Reg #28
    reg_tag_lo: u64,
    // Reg #29
    reg_tag_hi: u64,
    // Reg #30
    reg_error_epc: u64,

    logger: slog::Logger,
}

impl Cp0 {
    pub fn new(logger: slog::Logger) -> Box<Cp0> {
        Box::new(Cp0 {
            reg_index: 0,
            reg_random: 0,
            reg_entry_lo0: 0,
            reg_entry_lo1: 0,
            reg_context: 0,
            reg_page_mask: 0,
            reg_wired: 0,
            reg_bad_vaddr: 0,
            reg_count: 0,
            reg_entry_hi: 0,
            reg_compare: 0,
            reg_status: StatusReg::default(),
            reg_cause: 0,
            reg_epc: 0,
            reg_pr_id: 0,
            reg_config: 0,
            reg_ll_addr: 0,
            reg_watch_lo: 0,
            reg_watch_hi: 0,
            reg_x_context: 0,
            reg_parity_error: 0,
            reg_cache_error: 0,
            reg_tag_lo: 0,
            reg_tag_hi: 0,
            reg_error_epc: 0,

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
    fn fmt(&self) -> usize {
        (self.opcode & 0x1f) as usize
    }
}

impl Cop for Cp0 {
    fn reg(&self, idx: usize) -> u128 {
        match idx {
            CP0_REG_INDEX => self.reg_index as u128,
            CP0_REG_RANDOM => self.reg_random as u128,
            CP0_REG_ENTRY_LO0 => self.reg_entry_lo0 as u128,
            CP0_REG_ENTRY_LO1 => self.reg_entry_lo1 as u128,
            CP0_REG_CONTEXT => self.reg_context as u128,
            CP0_REG_PAGE_MASK => self.reg_page_mask as u128,
            CP0_REG_WIRED => self.reg_wired as u128,
            CP0_REG_BAD_VADDR => self.reg_bad_vaddr as u128,
            CP0_REG_COUNT => self.reg_count as u128,
            CP0_REG_ENTRY_HI => self.reg_entry_hi as u128,
            CP0_REG_COMPARE => self.reg_compare as u128,
            CP0_REG_STATUS => self.reg_status.0 as u128,
            CP0_REG_CAUSE => self.reg_cause as u128,
            CP0_REG_EPC => self.reg_epc as u128,
            CP0_REG_PRID => self.reg_pr_id as u128,
            CP0_REG_CONFIG => self.reg_config as u128,
            CP0_REG_LL_ADDR => self.reg_ll_addr as u128,
            CP0_REG_WATCH_LO => self.reg_watch_lo as u128,
            CP0_REG_WATCH_HI => self.reg_watch_hi as u128,
            CP0_REG_X_CONTEXT => self.reg_x_context as u128,
            CP0_REG_PARITY_ERROR => self.reg_parity_error as u128,
            CP0_REG_CACHE_ERROR => self.reg_cache_error as u128,
            CP0_REG_TAG_LO => self.reg_tag_lo as u128,
            CP0_REG_TAG_HI => self.reg_tag_hi as u128,
            CP0_REG_ERROR_EPC => self.reg_error_epc as u128,
            _ => {
                warn!(self.logger, "CP0 read reg: unknown register"; "reg" => idx);
                0
            }
        }
    }

    fn set_reg(&mut self, idx: usize, val: u128) {
        match idx {
            CP0_REG_INDEX => self.reg_index = val as u64,
            CP0_REG_RANDOM => self.reg_random = val as u64,
            CP0_REG_ENTRY_LO0 => self.reg_entry_lo0 = val as u64,
            CP0_REG_ENTRY_LO1 => self.reg_entry_lo1 = val as u64,
            CP0_REG_CONTEXT => self.reg_context = val as u64,
            CP0_REG_PAGE_MASK => self.reg_page_mask = val as u64,
            CP0_REG_WIRED => self.reg_wired = val as u64,
            CP0_REG_BAD_VADDR => self.reg_bad_vaddr = val as u64,
            CP0_REG_COUNT => self.reg_count = val as u64,
            CP0_REG_ENTRY_HI => self.reg_entry_hi = val as u64,
            CP0_REG_COMPARE => self.reg_compare = val as u64,
            CP0_REG_STATUS => self.reg_status.0 = val as u32,
            CP0_REG_CAUSE => self.reg_cause = val as u64,
            CP0_REG_EPC => self.reg_epc = val as u64,
            CP0_REG_PRID => self.reg_pr_id = val as u64,
            CP0_REG_CONFIG => self.reg_config = val as u64,
            CP0_REG_LL_ADDR => self.reg_ll_addr = val as u64,
            CP0_REG_WATCH_LO => self.reg_watch_lo = val as u64,
            CP0_REG_WATCH_HI => self.reg_watch_hi = val as u64,
            CP0_REG_X_CONTEXT => self.reg_x_context = val as u64,
            CP0_REG_PARITY_ERROR => self.reg_parity_error = val as u64,
            CP0_REG_CACHE_ERROR => self.reg_cache_error = val as u64,
            CP0_REG_TAG_LO => self.reg_tag_lo = val as u64,
            CP0_REG_TAG_HI => self.reg_tag_hi = val as u64,
            CP0_REG_ERROR_EPC => self.reg_error_epc = val as u64,
            _ => {
                warn!(self.logger, "CP0 write reg: unknown register"; "reg" => idx);
            }
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
                op.cpu.regs[op.rt()] = op.cop0.reg(op.rd()) as u64;
            }
            0x04 => {
                // MTC0 - write32
                if op.sel() == 0 {
                    let rd = op.rd();
                    let rt = op.rt64() as u128;
                    op.cop0.set_reg(rd, rt);
                } else {
                    warn!(
                        op.cop0.logger,
                        "unimplemented COP0 write32, sel is not 0";
                        "sel" => op.sel()
                    );
                }
            }
            0x10 => {
                // TLB
                match op.fmt() {
                    0x1 => {
                        // TLBR
                        warn!(op.cop0.logger, "unimplemented COP0 TLBR");
                    }
                    0x2 => {
                        // TLBWI
                        warn!(op.cop0.logger, "unimplemented COP0 TLBWI"; "opcode" => op.opcode);
                    }
                    0x6 => {
                        // TLBWR
                        warn!(op.cop0.logger, "unimplemented COP0 TLBWR");
                    }
                    0x18 => {
                        // ERET
                        info!(op.cop0.logger, "COP0 ERET"; "erl" => op.cop0.reg_status.erl(), "epc" => op.cop0.reg_epc);

                        if op.cop0.reg_status.erl() {
                            op.cpu.pc = op.cop0.reg_error_epc as u32;
                            op.cop0.reg_status.set_erl(false);
                        } else {
                            op.cpu.pc = op.cop0.reg_epc as u32;
                            op.cop0.reg_status.set_exl(false);
                        }
                    }
                    _ => panic!(
                        "unimplemented COP0 opcode (TLB section): func={:x?} fmt={:x?}",
                        op.func(),
                        op.fmt(),
                    ),
                }
            }
            _ => panic!("unimplemented COP0 opcode: func={:x?}", op.func()),
        }
    }
}
