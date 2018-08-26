use super::cpu::{Cop, Cop0, CpuContext, Exception};
use super::segment::Segment;
use super::tlb;
use rand::{FromEntropy, Rng, XorShiftRng};
use slog;

const EXC_LOC_COMMON: u32 = 0xBFC0_0000;
const EXC_LOC_BASE_0: u32 = 0x8000_0000;
const EXC_LOC_BASE_1: u32 = 0xBFC0_0000;
const EXC_LOC_OFF_TLB_MISS: u32 = 0x0000;
const EXC_LOC_OFF_XTLB_MISS: u32 = 0x0080;
const EXC_LOC_OFF_OTHER: u32 = 0x0180;

// Only safe in single threaded environements!
static mut RNG: Option<XorShiftRng> = None;
unsafe fn rng() -> &'static mut XorShiftRng {
    if RNG.is_none() {
        RNG = Some(XorShiftRng::from_entropy());
    }

    RNG.as_mut().unwrap()
}

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
    pub exl, set_exl: 1;

    // Specifies and indicates error level
    // (0 - normal, 1 - error)
    pub erl, set_erl: 2;

    // Specifies and indicates mode bits
    // (10 - User, 01 - Supervisor, 00 - Kernel)
    pub ksu, set_ksu: 4, 3;

    // Enables 64-bit addressing and operations in User mode. When this bit is set, XTLB
    // miss exception is generated on TLB misses in User mode addresses space.
    // (0 - 32-bit, 1 - 64-bit)
    pub ux, set_ux: 5;

    // Enables 64-bit addressing and operations in Supervisor mode. When this bit is set, XTLB
    // miss exception is generated on TLB misses in Supervisor mode addresses space.
    // (0 - 32-bit, 1 - 64-bit)
    pub sx, set_sx: 6;

    // Enables 64-bit addressing in Kernel mode. When this bit is set, XTLB
    // miss exception is generated on TLB misses in Kernel mode addresses space.
    // (0 - 32-bit, 1 - 64-bit)
    // 64-bit operation is always valid in Kernel mode.
    pub kx, set_kx: 7;

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
    ds_ts, set_ds_ts: 21;

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

bitflags! {
    struct EXC_CODE: u8 {
        /// Exception Code - Interrupt
        const INT = 0;
        /// Exception Code - TLB Modification exception
        const MOD = 1;
        /// Exception Code - TLB Miss exception (load or instruction fetch)
        const TLBL = 2;
        /// Exception Code - TLB Miss exception (store)
        const TLBS = 3;
        /// Exception Code - Address Error exception (load or instruction fetch)
        const ADEL = 4;
        /// Exception Code - Address Error exception (store)
        const ADES = 5;
        /// Exception Code - Bus Error exception (instruction fetch)
        const IBE = 6;
        /// Exception Code - Bus Error exception (data reference: load or store)
        const DBE = 7;
        /// Exception Code - Syscall exception
        const SYS = 8;
        /// Exception Code - Breakpoint exception
        const BP = 9;
        /// Exception Code - Reserved Instruction exception
        const RI = 10;
        /// Exception Code - Coprocessor Unusable exception
        const CPU = 11;
        /// Exception Code - Arithmetic Overflow exception
        const OV = 12;
        /// Exception Code - Trap exception
        const TR = 13;
        /// Exception Code - Floating-Point exception
        const FPE = 15;
        /// Exception Code - Watch exception
        const WATCH = 23;
    }
}

bitfield! {
    #[derive(Default)]
    pub struct CauseReg(u32);
    impl Debug;

    /// Exception Code
    u8, exc_code, set_exc_code: 6, 2;
    /// Interrupt pending?
    u8, ip, set_ip: 15, 8;
    /// Coprocessor unit, only defined for coprocessor unusable exceptions.
    u8, ce, set_ce: 29, 28;
    /// Exception occured in branch delay slot?
    bd, set_bd: 31;
}

/// System Control Coprocessor (CP0)
pub struct Cp0 {
    /// [tlb] #0 Index
    reg_index: u32,
    /// [tbl] #2 Entry Lo0 (even)
    reg_entry_lo0: u64,
    /// [tlb] #3 Entry Lo1 (odd)
    reg_entry_lo1: u64,
    /// [tlb/exception] #4 Context
    /// Used in handling TLB Miss exceptions.
    /// Contains PTEBase || BadVPN2 on exception || 0000
    reg_context: u64,
    /// [tlb] #5 Page Mask
    reg_page_mask: u32,
    /// [tlb] #6 Wired
    /// Protects parts of the tlb from being overwritten in TLBWR.
    reg_wired: u32,
    /// [exception] #8 Bad Virtual Address
    /// Read only. Holds the last virtual address, which errored out.
    reg_bad_vaddr: u64,
    /// [exception] #9 Count
    /// Running counter at half the clock speed of PClock.
    reg_count: u64,
    /// [tlb] #10 Entry Hi
    reg_entry_hi: u64,
    /// [exception] #11 Compare
    /// Used for comparision with the count register to generate a timer interrupt.
    reg_compare: u64,
    /// [exception] #12 Status
    /// Hold various status informations, see the definition of `StatusReg` for details.
    reg_status: StatusReg,
    /// [exception] #13 Cause
    /// Hold the cause of the last exception occured
    reg_cause: CauseReg,
    /// [exception] #14 Exception Program Counter
    /// Contains either the vaddr of the instruction that was the direct cause
    /// or the vaddr of the immediately preceding branch or jump instruction
    /// (if the direct cause was inside the branch delay slot).
    /// This is the address where execution is resumed after processing.
    reg_epc: u64,
    /// Reg #15
    reg_pr_id: u64,
    // Reg #16
    reg_config: u64,
    // Reg #17
    reg_ll_addr: u64,
    /// [exception] #18 Watch Lo
    reg_watch_lo: u64,
    /// [exception] #19 Watch Hi
    reg_watch_hi: u64,
    /// [exception] #20 XContext
    /// 64bit version of Context.
    reg_x_context: u64,
    /// [exception] #26 Parity Error
    /// Unused.
    reg_parity_error: u64,
    /// [exception] #27 Cache Error
    /// Read only. Unused.
    reg_cache_error: u64,
    // Reg #28
    reg_tag_lo: u64,
    // Reg #29
    reg_tag_hi: u64,
    /// [exception] #30 Error Exception Program Counter
    /// Similar to EPC, also used to store the PC on {Cold|Soft} Reset and NMI.
    reg_error_epc: u64,

    tlb: tlb::Tlb,
    logger: slog::Logger,
}

impl Cp0 {
    pub fn new(logger: slog::Logger) -> Box<Cp0> {
        let mut reg_status = StatusReg::default();
        // set defaults for cold reset
        reg_status.set_erl(true);
        reg_status.set_ds_bev(true);

        Box::new(Cp0 {
            reg_index: 0,
            reg_entry_lo0: 0,
            reg_entry_lo1: 0,
            reg_context: 0,
            reg_page_mask: 0,
            reg_wired: 0,
            reg_bad_vaddr: 0,
            reg_count: 0,
            reg_entry_hi: 0,
            reg_compare: 0,
            reg_status,
            reg_cause: CauseReg::default(),
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

            tlb: tlb::Tlb::default(),
            logger: logger,
        })
    }

    /// Fetches the matching memory segment for the given virtual address.
    fn get_segment(&self, vaddr: u64) -> &Segment {
        Segment::from_vaddr(vaddr, &self.reg_status)
    }

    fn exception_setup(&mut self, ctx: &mut CpuContext, exc: Exception) {
        if let Some(cause) = EXC_CODE::from_bits(exc.as_u8()) {
            // These are all the exceptions that are not RESET and NMI.

            self.reg_cause.set_exc_code(cause.bits() as u8);

            if ctx.branch_pc != 0 {
                // In a branch delay, use the preceeding branch as resume point.
                self.reg_epc = (ctx.branch_pc - 4) as u64;
                self.reg_cause.set_bd(true);
            } else {
                self.reg_epc = ctx.pc as u64;
            }
        }
    }
}

impl Cop0 for Cp0 {
    fn pending_int(&self) -> bool {
        false
    }

    fn exception(&mut self, ctx: &mut CpuContext, exc: Exception) {
        let next_pc: u32;

        match exc {
            Exception::INT => unimplemented!("INT Exception"),
            Exception::MOD => unimplemented!("TLB MOD Exception"),
            Exception::TLBL_MISS | Exception::TLBS_MISS => {
                self.exception_setup(ctx, exc);

                let base = if self.reg_status.exl() {
                    EXC_LOC_COMMON
                } else if self.reg_status.ds_bev() {
                    EXC_LOC_BASE_1
                } else {
                    EXC_LOC_BASE_0
                };
                let offset = if self.reg_status.ux() || self.reg_status.sx() || self.reg_status.kx()
                {
                    // 64bit
                    EXC_LOC_OFF_XTLB_MISS
                } else {
                    // 32bit
                    EXC_LOC_OFF_TLB_MISS
                };

                next_pc = base + offset;
            }
            Exception::TLBL_INVALID | Exception::TLBS_INVALID => {
                self.exception_setup(ctx, exc);
                next_pc = EXC_LOC_COMMON + EXC_LOC_OFF_OTHER;
            }
            Exception::ADEL => unimplemented!("ADEL Exception"),
            Exception::ADES => unimplemented!("ADES Exception"),
            Exception::IBE => unimplemented!("IBE Exception"),
            Exception::DBE => unimplemented!("DBE Exception"),
            Exception::SYS => unimplemented!("SYS Exception"),
            Exception::BP => unimplemented!("BP Exception"),
            Exception::RI => unimplemented!("RI Exception"),
            Exception::TR => unimplemented!("TR Exception"),
            Exception::FPE => unimplemented!("FPE Exception"),
            Exception::WATCH => unimplemented!("WATCH Exception"),

            // Special exceptions that are not specified in the Cause register
            Exception::RESET => {
                next_pc = EXC_LOC_COMMON;

                self.reg_status.set_ds_ts(false);
                self.reg_status.set_ds_sr(false);
                self.reg_status.set_rp(false);
                self.reg_status.set_erl(true);
                self.reg_status.set_ds_bev(true);

                // TODO: set RegConfig BE = 1
                // TODO: set RegConfig EP(3:0) = 0
                // TODO: set RegConfig EC(2:0) = DivMode(2:0)
            }
            Exception::SOFTRESET => {
                next_pc = if !self.reg_status.erl() {
                    self.reg_error_epc as u32
                } else {
                    EXC_LOC_COMMON
                };

                self.reg_status.set_ds_ts(false);
                self.reg_status.set_rp(false);
                self.reg_status.set_erl(true);
                self.reg_status.set_ds_bev(true);
                self.reg_status.set_ds_sr(true);
            }
            Exception::NMI => {
                next_pc = self.reg_error_epc as u32;

                self.reg_status.set_ds_ts(false);
                self.reg_status.set_erl(true);
                self.reg_status.set_ds_bev(true);
                self.reg_status.set_ds_sr(true);
            }
        }

        ctx.pc = next_pc;
    }

    fn translate_addr(&mut self, vaddr: u64) -> Result<u32, Exception> {
        let segment = self.get_segment(vaddr);

        if segment.mapped {
            info!(self.logger, "reading tlb mapped address region"; "vaddr" => format!("{:#0x}", vaddr));
            let asid = self.reg_entry_hi as u8;
            let index = self.tlb.probe(vaddr, asid);

            if let Some(index) = index {
                let entry = self.tlb.read(index);
                let page_mask = (entry.page_mask | 0x1FFF) >> 1;
                // this selects the first bit after the page mask (bit 13 for 4KB page size)
                let is_odd = vaddr & (page_mask as u64 + 1) != 0;
                if (!is_odd && entry.valid0()) || (is_odd && entry.valid1()) {
                    if is_odd {
                        Ok(entry.pfn1() | (vaddr as u32 & page_mask))
                    } else {
                        Ok(entry.pfn0() | (vaddr as u32 & page_mask))
                    }
                } else {
                    // TODO: set
                    // - badvaddr
                    // - context
                    // - xcontext
                    // - entryhi
                    Err(Exception::TLBL_INVALID)
                    // panic!("TLB INVALID not yet handled: {:#0x}, {:#0x}", vaddr, asid);
                }
            } else {
                // TODO: set
                // - badvaddr
                // - context
                // - xcontext
                // - entryhi
                Err(Exception::TLBL_MISS)
                // panic!("TLB MISS not yet handled: {:#0x}, {:#0x}", vaddr, asid);
            }
        } else {
            Ok((vaddr - segment.start) as u32)
        }
    }
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
            CP0_REG_RANDOM => {
                // The value is technicaly generated as backwords counter,
                // which is decremented on every cpu cycle.
                // But even cen64 uses a random number generator for this, so we should
                // be good to do this too for now.
                // The important part is that we generate a number in the range of
                // [wired, 32).

                let val: u32 = unsafe { rng().gen_range(self.reg_wired.into(), 32) };
                val as u128
            }
            CP0_REG_ENTRY_LO0 => self.reg_entry_lo0 as u128,
            CP0_REG_ENTRY_LO1 => self.reg_entry_lo1 as u128,
            CP0_REG_CONTEXT => self.reg_context as u128,
            CP0_REG_PAGE_MASK => self.reg_page_mask as u128,
            CP0_REG_WIRED => self.reg_wired as u128,
            CP0_REG_BAD_VADDR => self.reg_bad_vaddr as u128,
            CP0_REG_COUNT => {
                error!(self.logger, "(read) reg count is not yet implemented");
                0
            }
            CP0_REG_ENTRY_HI => self.reg_entry_hi as u128,
            CP0_REG_COMPARE => self.reg_compare as u128,
            CP0_REG_STATUS => self.reg_status.0 as u128,
            CP0_REG_CAUSE => self.reg_cause.0 as u128,
            CP0_REG_EPC => self.reg_epc as u128,
            CP0_REG_PRID => self.reg_pr_id as u128,
            CP0_REG_CONFIG => self.reg_config as u128,
            CP0_REG_LL_ADDR => self.reg_ll_addr as u128,
            CP0_REG_WATCH_LO => {
                error!(self.logger, "(read) watch lo is not yet implemented");
                self.reg_watch_lo as u128
            }
            CP0_REG_WATCH_HI => {
                error!(self.logger, "(read) watch hi is not yet implemented");
                self.reg_watch_hi as u128
            }
            CP0_REG_X_CONTEXT => {
                error!(self.logger, "(read) xcontext is not yet implemented");
                self.reg_x_context as u128
            }
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
            CP0_REG_INDEX => self.reg_index = val as u32,
            CP0_REG_RANDOM => panic!("reg random is readonly"),
            CP0_REG_ENTRY_LO0 => self.reg_entry_lo0 = val as u64,
            CP0_REG_ENTRY_LO1 => self.reg_entry_lo1 = val as u64,
            CP0_REG_CONTEXT => self.reg_context = val as u64,
            CP0_REG_PAGE_MASK => self.reg_page_mask = val as u32,
            CP0_REG_WIRED => self.reg_wired = val as u32,
            CP0_REG_BAD_VADDR => panic!("reg bad vaddr is readonly"),
            CP0_REG_COUNT => {
                error!(self.logger, "(write) reg count is not yet implemented");
                self.reg_count = val as u64;
            }
            CP0_REG_ENTRY_HI => self.reg_entry_hi = val as u64,
            CP0_REG_COMPARE => self.reg_compare = val as u64,
            CP0_REG_STATUS => self.reg_status.0 = val as u32,
            CP0_REG_CAUSE => self.reg_cause.0 = val as u32,
            CP0_REG_EPC => self.reg_epc = val as u64,
            CP0_REG_PRID => self.reg_pr_id = val as u64,
            CP0_REG_CONFIG => self.reg_config = val as u64,
            CP0_REG_LL_ADDR => self.reg_ll_addr = val as u64,
            CP0_REG_WATCH_LO => {
                error!(self.logger, "(write) watch lo is not yet implemented");
                self.reg_watch_lo = val as u64;
            }
            CP0_REG_WATCH_HI => {
                error!(self.logger, "(write) watch hi is not yet implemented");
                self.reg_watch_hi = val as u64;
            }
            CP0_REG_X_CONTEXT => {
                error!(self.logger, "(write) xcontext is not yet implemented");
                self.reg_x_context = val as u64
            }
            CP0_REG_PARITY_ERROR => self.reg_parity_error = val as u64,
            CP0_REG_CACHE_ERROR => panic!("reg cache error is readonly"),
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

                        // TODO: this could fail, should handle this somehow
                        let entry = op.cop0.tlb.read((op.cop0.reg_index & 0x3F) as usize);

                        // information is written into the various registers
                        op.cop0.reg_entry_hi = entry.hi();
                        op.cop0.reg_entry_lo0 = entry.lo0;
                        op.cop0.reg_entry_lo1 = entry.lo1;
                        op.cop0.reg_page_mask = entry.page_mask & 0x1FFF_E000;
                    }
                    0x2 => {
                        // TLBWI
                        let index = (op.cop0.reg_index & 0x3F) as usize;
                        let entry_hi = op.cop0.reg_entry_hi;

                        info!(op.cop0.logger, "TLBWI"; "index" => index, "entry_hi" => format!("{:#0x}", entry_hi));

                        op.cop0.tlb.write(
                            index,
                            op.cop0.reg_page_mask,
                            entry_hi,
                            op.cop0.reg_entry_lo0,
                            op.cop0.reg_entry_lo1,
                        );
                    }
                    0x6 => {
                        // TLBWR
                        let index = op.cop0.reg(CP0_REG_RANDOM) as usize;
                        let entry_hi = op.cop0.reg_entry_hi;

                        info!(op.cop0.logger, "TLBWR"; "index" => index, "entry_hi" => entry_hi);

                        op.cop0.tlb.write(
                            index,
                            op.cop0.reg_page_mask,
                            entry_hi,
                            op.cop0.reg_entry_lo0,
                            op.cop0.reg_entry_lo1,
                        );
                    }
                    0x18 => {
                        // ERET
                        info!(op.cop0.logger, "COP0 ERET"; "erl" => op.cop0.reg_status.erl(), "epc" => op.cop0.reg_epc);

                        if op.cop0.reg_status.erl() {
                            op.cpu.set_pc(op.cop0.reg_error_epc as u32);
                            op.cop0.reg_status.set_erl(false);
                        } else {
                            op.cpu.set_pc(op.cop0.reg_epc as u32);
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
