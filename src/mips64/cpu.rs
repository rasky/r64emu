extern crate emu;

use self::emu::bus::be::{Bus, MemIoR};
use self::emu::bus::MemInt;
use self::emu::int::Numerics;
use self::emu::sync;
use super::cop0::Cop0;
use super::Mipsop;
use slog;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Cpu {
    pub(crate) regs: [u64; 32],
    hi: u64,
    lo: u64,

    pub(crate) cop0: Cop0,
    pub(crate) logger: slog::Logger,
    bus: Rc<RefCell<Box<Bus>>>,
    pub(crate) pc: u32,
    branch_pc: u32,
    clock: i64,
    until: i64,
    pub(crate) tight_exit: bool,

    last_fetch_addr: u32,
    last_fetch_mem: MemIoR<u32>,
}

macro_rules! branch {
    ($op:ident, $cond:expr, $tgt:expr) => {{
        branch!($op, $cond, $tgt, link(false), likely(false));
    }};
    ($op:ident, $cond:expr, $tgt:expr,link($link:expr)) => {{
        branch!($op, $cond, $tgt, link($link), likely(true));
    }};
    ($op:ident, $cond:expr, $tgt:expr,likely($lkl:expr)) => {{
        branch!($op, $cond, $tgt, link(false), likely($lkl));
    }};
    ($op:ident, $cond:expr, $tgt:expr,link($link:expr),likely($lkl:expr)) => {{
        if $link {
            $op.cpu.regs[31] = ($op.cpu.pc + 4) as u64;
        }
        let (cond, tgt) = ($cond, $tgt);
        $op.cpu.branch(cond, tgt, $lkl);
    }};
}

macro_rules! check_overflow_add {
    ($op:ident, $dest:expr, $reg1:expr, $reg2:expr) => {{
        match $reg1.checked_add($reg2) {
            Some(res) => $dest = res.sx64(),
            None => $op.cpu.trap_overflow(),
        }
    }};
}

macro_rules! check_overflow_sub {
    ($op:ident, $dest:expr, $reg1:expr, $reg2:expr) => {{
        match $reg1.checked_sub($reg2) {
            Some(res) => $dest = res.sx64(),
            None => $op.cpu.trap_overflow(),
        }
    }};
}

impl Cpu {
    pub fn new(logger: slog::Logger, bus: Rc<RefCell<Box<Bus>>>) -> Cpu {
        return Cpu {
            bus: bus,
            logger: logger,
            regs: [0u64; 32],
            cop0: Cop0::default(),
            hi: 0,
            lo: 0,
            pc: 0x1FC0_0000, // FIXME
            branch_pc: 0,
            clock: 0,
            until: 0,
            tight_exit: false,
            last_fetch_addr: 0,
            last_fetch_mem: MemIoR::default(),
        };
    }

    pub fn get_pc(&self) -> u32 {
        self.pc
    }

    fn trap_overflow(&mut self) {
        unimplemented!();
    }

    fn cop(&mut self, idx: usize, opcode: u32) {
        let mut op = Mipsop { opcode, cpu: self };
        error!(op.cpu.logger, "unimplemented COP opcode"; o!("cop" => idx, "op" => op.hex(), "func" => op.rs()));
        unimplemented!();
    }

    fn op(&mut self, opcode: u32) {
        self.clock += 1;
        let mut op = Mipsop { opcode, cpu: self };
        match op.op() {
            // SPECIAL
            0x00 => match op.special() {
                0x00 => *op.mrd64() = (op.rt32() << op.sa()).sx64(), // SLL
                0x02 => *op.mrd64() = (op.rt32() >> op.sa()).sx64(), // SRL
                0x03 => *op.mrd64() = (op.irt32() >> op.sa()).sx64(), // SRA
                0x04 => *op.mrd64() = (op.rt32() << (op.rs32() & 0x1F)).sx64(), // SLLV
                0x06 => *op.mrd64() = (op.rt32() >> (op.rs32() & 0x1F)).sx64(), // SRLV
                0x07 => *op.mrd64() = (op.irt32() >> (op.rs32() & 0x1F)).sx64(), // SRAV
                0x08 => branch!(op, true, op.rs32(), link(false)),   // JR
                0x09 => branch!(op, true, op.rs32(), link(true)),    // JALR
                0x0F => {}                                           // SYNC

                0x10 => *op.mrd64() = op.cpu.hi, // MFHI
                0x11 => op.cpu.hi = op.rs64(),   // MTHI
                0x12 => *op.mrd64() = op.cpu.lo, // MFLO
                0x13 => op.cpu.lo = op.rs64(),   // MTLO
                0x14 => *op.mrd64() = op.rt64() << (op.rs32() & 0x3F), // DSLLV
                0x16 => *op.mrd64() = op.rt64() >> (op.rs32() & 0x3F), // DSRLV
                0x17 => *op.mrd64() = (op.irt64() >> (op.rs32() & 0x3F)) as u64, // DSRAV
                0x18 => {
                    // MULT
                    let (hi, lo) =
                        (i64::wrapping_mul(op.rt32().isx64(), op.rs32().isx64()) as u64).hi_lo();
                    op.cpu.lo = lo;
                    op.cpu.hi = hi;
                }
                0x19 => {
                    // MULTU
                    let (hi, lo) = u64::wrapping_mul(op.rt32() as u64, op.rs32() as u64).hi_lo();
                    op.cpu.lo = lo;
                    op.cpu.hi = hi;
                }
                0x1A => {
                    // DIV
                    op.cpu.lo = op.irs32().wrapping_div(op.irt32()).sx64();
                    op.cpu.hi = op.irs32().wrapping_rem(op.irt32()).sx64();
                }
                0x1B => {
                    // DIVU
                    op.cpu.lo = op.rs32().wrapping_div(op.rt32()).sx64();
                    op.cpu.hi = op.rs32().wrapping_rem(op.rt32()).sx64();
                }
                0x1C => {
                    // DMULT
                    let (hi, lo) =
                        i128::wrapping_mul(op.irt64() as i128, op.irs64() as i128).hi_lo();
                    op.cpu.lo = lo as u64;
                    op.cpu.hi = hi as u64;
                }
                0x1D => {
                    // DMULTU
                    let (hi, lo) = u128::wrapping_mul(op.rt64() as u128, op.rs64() as u128).hi_lo();
                    op.cpu.lo = lo as u64;
                    op.cpu.hi = hi as u64;
                }
                0x1E => {
                    // DDIV
                    op.cpu.lo = op.irs64().wrapping_div(op.irt64()) as u64;
                    op.cpu.hi = op.irs64().wrapping_rem(op.irt64()) as u64;
                }
                0x1F => {
                    // DDIVU
                    op.cpu.lo = op.rs64().wrapping_div(op.rt64());
                    op.cpu.hi = op.rs64().wrapping_rem(op.rt64());
                }

                0x20 => check_overflow_add!(op, *op.mrd64(), op.irs32(), op.irt32()), // ADD
                0x21 => *op.mrd64() = (op.rs32() + op.rt32()).sx64(),                 // ADDU
                0x22 => check_overflow_sub!(op, *op.mrd64(), op.irs32(), op.irt32()), // SUB
                0x23 => *op.mrd64() = (op.rs32() - op.rt32()).sx64(),                 // SUBU
                0x24 => *op.mrd64() = op.rs64() & op.rt64(),                          // AND
                0x25 => *op.mrd64() = op.rs64() | op.rt64(),                          // OR
                0x26 => *op.mrd64() = op.rs64() ^ op.rt64(),                          // XOR
                0x27 => *op.mrd64() = !(op.rs64() | op.rt64()),                       // NOR
                0x2A => *op.mrd64() = (op.irs32() < op.irt32()) as u64,               // SLT
                0x2B => *op.mrd64() = (op.rs32() < op.rt32()) as u64,                 // SLTU
                0x2C => check_overflow_add!(op, *op.mrd64(), op.irs64(), op.irt64()), // DADD
                0x2D => *op.mrd64() = op.rs64() + op.rt64(),                          // DADDU
                0x2E => check_overflow_sub!(op, *op.mrd64(), op.irs64(), op.irt64()), // DSUB
                0x2F => *op.mrd64() = op.rs64() - op.rt64(),                          // DSUBU

                0x38 => *op.mrd64() = op.rt64() << op.sa(), // DSLL
                0x3A => *op.mrd64() = op.rt64() >> op.sa(), // DSRL
                0x3B => *op.mrd64() = (op.irt64() >> op.sa()) as u64, // DSRA
                0x3C => *op.mrd64() = op.rt64() << (op.sa() + 32), // DSLL32
                0x3E => *op.mrd64() = op.rt64() >> (op.sa() + 32), // DSRL32
                0x3F => *op.mrd64() = (op.irt64() >> (op.sa() + 32)) as u64, // DSRA32

                _ => panic!("unimplemented special opcode: func=0x{:x?}", op.special()),
            },

            // REGIMM
            0x01 => match op.rt() {
                0x00 => branch!(op, op.irs64() < 0, op.btgt(), link(false), likely(false)), // BLTZ
                0x01 => branch!(op, op.irs64() >= 0, op.btgt(), link(false), likely(false)), // BGEZ
                0x02 => branch!(op, op.irs64() < 0, op.btgt(), link(false), likely(true)),  // BLTZL
                0x03 => branch!(op, op.irs64() >= 0, op.btgt(), link(false), likely(true)), // BGEZL
                0x10 => branch!(op, op.irs64() < 0, op.btgt(), link(true), likely(false)), // BLTZAL
                0x11 => branch!(op, op.irs64() >= 0, op.btgt(), link(true), likely(false)), // BGEZAL
                0x12 => branch!(op, op.irs64() < 0, op.btgt(), link(true), likely(true)), // BLTZALL
                0x13 => branch!(op, op.irs64() >= 0, op.btgt(), link(true), likely(true)), // BGEZALL
                _ => panic!("unimplemented regimm opcode: func=0x{:x?}", op.rt()),
            },

            0x02 => branch!(op, true, op.jtgt(), link(false)), // J
            0x03 => branch!(op, true, op.jtgt(), link(true)),  // JAL
            0x04 => branch!(op, op.rs64() == op.rt64(), op.btgt()), // BEQ
            0x05 => branch!(op, op.rs64() != op.rt64(), op.btgt()), // BNE
            0x06 => branch!(op, op.irs64() <= 0, op.btgt()),   // BLEZ
            0x07 => branch!(op, op.irs64() > 0, op.btgt()),    // BGTZ
            0x08 => check_overflow_add!(op, *op.mrt64(), op.irs32(), op.sximm32()), // ADDI
            0x09 => *op.mrt64() = (op.irs32() + op.sximm32()).sx64(), // ADDIU
            0x0A => *op.mrt64() = (op.irs32() < op.sximm32()) as u64, // SLTI
            0x0B => *op.mrt64() = (op.rs32() < op.sximm32() as u32) as u64, // SLTIU
            0x0C => *op.mrt64() = op.rs64() & op.imm64(),      // ANDI
            0x0D => *op.mrt64() = op.rs64() | op.imm64(),      // ORI
            0x0E => *op.mrt64() = op.rs64() ^ op.imm64(),      // XORI
            0x0F => *op.mrt64() = (op.sximm32() << 16).sx64(), // LUI

            0x10 => Cop0::op(op.cpu, opcode), // COP0
            0x11 => op.cpu.cop(1, opcode),    // COP1
            0x12 => op.cpu.cop(2, opcode),    // COP2
            0x13 => op.cpu.cop(3, opcode),    // COP3
            0x14 => branch!(op, op.rs64() == op.rt64(), op.btgt(), likely(true)), // BEQL
            0x15 => branch!(op, op.rs64() != op.rt64(), op.btgt(), likely(true)), // BNEL
            0x16 => branch!(op, op.irs64() <= 0, op.btgt(), likely(true)), // BLEZL
            0x17 => branch!(op, op.irs64() > 0, op.btgt(), likely(true)), // BGTZL
            0x18 => check_overflow_add!(op, *op.mrt64(), op.irs64(), op.sximm64()), // DADDI
            0x19 => *op.mrt64() = (op.irs64() + op.sximm64()) as u64, // DADDIU

            0x20 => *op.mrt64() = op.cpu.read::<u8>(op.ea()).sx64(), // LB
            0x21 => *op.mrt64() = op.cpu.read::<u16>(op.ea()).sx64(), // LH
            0x22 => *op.mrt64() = op.cpu.lwl(op.ea(), op.rt32()).sx64(), // LWL
            0x23 => *op.mrt64() = op.cpu.read::<u32>(op.ea()).sx64(), // LW
            0x24 => *op.mrt64() = op.cpu.read::<u8>(op.ea()) as u64, // LBU
            0x25 => *op.mrt64() = op.cpu.read::<u16>(op.ea()) as u64, // LHU
            0x26 => *op.mrt64() = op.cpu.lwr(op.ea(), op.rt32()).sx64(), // LWR
            0x27 => *op.mrt64() = op.cpu.read::<u32>(op.ea()) as u64, // LWU
            0x28 => op.cpu.write::<u8>(op.ea(), op.rt32() as u8),    // SB
            0x29 => op.cpu.write::<u16>(op.ea(), op.rt32() as u16),  // SH
            0x2A => op.cpu.write::<u32>(op.ea(), op.cpu.swl(op.ea(), op.rt32())), // SWL
            0x2B => op.cpu.write::<u32>(op.ea(), op.rt32()),         // SW
            0x2E => op.cpu.write::<u32>(op.ea(), op.cpu.swr(op.ea(), op.rt32())), // SWR
            0x2F => {}                                               // CACHE

            0x37 => *op.mrt64() = op.cpu.read::<u64>(op.ea()), // LD
            0x3F => op.cpu.write::<u64>(op.ea(), op.rt64()),   // SD

            _ => panic!(
                "unimplemented opcode: func=0x{:x?}, pc={}",
                op.op(),
                op.cpu.pc.hex()
            ),
        }
    }

    fn lwl(&self, addr: u32, reg: u32) -> u32 {
        let mem = self.read::<u32>(addr);
        let shift = (addr & 3) * 8;
        let mask = (1 << shift) - 1;
        (reg & mask) | ((mem << shift) & !mask)
    }

    fn lwr(&self, addr: u32, reg: u32) -> u32 {
        let mem = self.read::<u32>(addr);
        let shift = (!addr & 3) * 8;
        let mask = ((1u64 << (32 - shift)) - 1) as u32;
        (reg & !mask) | ((mem >> shift) & mask)
    }

    fn swl(&self, addr: u32, reg: u32) -> u32 {
        let mem = self.read::<u32>(addr);
        let shift = (addr & 3) * 8;
        let mask = ((1u64 << (32 - shift)) - 1) as u32;
        (mem & !mask) | ((reg >> shift) & mask)
    }

    fn swr(&self, addr: u32, reg: u32) -> u32 {
        let mem = self.read::<u32>(addr);
        let shift = (!addr & 3) * 8;
        let mask = (1 << shift) - 1;
        (mem & mask) | ((reg << shift) & !mask)
    }

    fn fetch(&mut self, addr: u32) -> &MemIoR<u32> {
        // Save last fetched memio, to speed up hot loops
        if self.last_fetch_addr != addr {
            self.last_fetch_addr = addr;
            self.last_fetch_mem = self.bus.borrow().fetch_read::<u32>(addr & 0x1FFF_FFFC);
        }
        &self.last_fetch_mem
    }

    fn read<U: MemInt>(&self, addr: u32) -> U {
        self.bus
            .borrow()
            .read::<U>(addr & 0x1FFF_FFFF & !(U::SIZE as u32 - 1))
    }

    fn write<U: MemInt>(&self, addr: u32, val: U) {
        self.bus
            .borrow()
            .write::<U>(addr & 0x1FFF_FFFF & !(U::SIZE as u32 - 1), val);
    }

    #[inline]
    fn branch(&mut self, cond: bool, tgt: u32, likely: bool) {
        if cond {
            self.branch_pc = tgt;
            self.tight_exit = true;
        } else if likely {
            // branch not taken; if likely, skip delay slot
            self.pc += 4;
            self.clock += 1;
            self.tight_exit = true;
        }
    }

    pub fn run(&mut self, until: i64) {
        self.until = until;
        while self.clock < self.until {
            let pc = self.pc;
            let mut iter = self.fetch(pc).iter().unwrap();

            // Tight loop: go through continuous memory, no branches, no IRQs
            self.tight_exit = false;
            while let Some(op) = iter.next() {
                self.pc += 4;
                self.op(op);
                if self.clock >= self.until || self.tight_exit {
                    break;
                }
            }

            if self.branch_pc != 0 {
                let pc = self.pc;
                let op = iter.next().unwrap_or_else(|| self.fetch(pc).read());
                self.pc = self.branch_pc;
                self.branch_pc = 0;
                self.op(op);
            }
        }
    }
}

impl sync::Subsystem for Box<Cpu> {
    fn run(&mut self, until: i64) {
        Cpu::run(self, until)
    }

    fn cycles(&self) -> i64 {
        self.clock
    }
}
