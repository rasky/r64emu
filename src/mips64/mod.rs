extern crate num;

mod cop0;
use self::cop0::Cop0;

use super::emu::bus::be::{Bus, MemIoR};
use super::emu::bus::MemInt;
use slog;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

trait Hex {
    fn hex(&self) -> String;
}

impl Hex for u32 {
    fn hex(&self) -> String {
        format!("0x{:x}", *self)
    }
}

impl Hex for u64 {
    fn hex(&self) -> String {
        format!("0x{:x}", *self)
    }
}

trait Numerics64 {
    fn hi_lo(self) -> (u64, u64);
}

impl Numerics64 for i64 {
    fn hi_lo(self) -> (u64, u64) {
        return (self as u64).hi_lo();
    }
}

impl Numerics64 for u64 {
    fn hi_lo(self) -> (u64, u64) {
        return (self & 0xffffffff, self >> 32);
    }
}

trait Numerics32 {
    fn isx64(&self) -> i64;
    fn sx64(&self) -> u64;
}

impl Numerics32 for u32 {
    fn isx64(&self) -> i64 {
        *self as i32 as i64
    }
    fn sx64(&self) -> u64 {
        *self as i32 as i64 as u64
    }
}

impl Numerics32 for i32 {
    fn isx64(&self) -> i64 {
        *self as i64
    }
    fn sx64(&self) -> u64 {
        *self as i64 as u64
    }
}

struct Mipsop<'a> {
    opcode: u32,
    cpu: &'a mut Cpu,
}

impl<'a> Mipsop<'a> {
    fn op(&self) -> u32 {
        self.opcode >> 26
    }
    fn special(&self) -> u32 {
        self.opcode & 0x3f
    }
    fn sel(&self) -> u32 {
        self.opcode & 7
    }
    fn ea(&self) -> u32 {
        self.rs32() + self.sximm32() as u32
    }
    fn btgt(&self) -> u32 {
        self.cpu.pc + self.sximm32() as u32 * 4
    }
    fn rs(&self) -> usize {
        ((self.opcode >> 21) & 0x1f) as usize
    }
    fn rt(&self) -> usize {
        ((self.opcode >> 16) & 0x1f) as usize
    }
    fn rd(&self) -> usize {
        ((self.opcode >> 11) & 0x1f) as usize
    }
    fn sximm32(&self) -> i32 {
        (self.opcode & 0xffff) as i16 as i32
    }
    fn imm64(&self) -> u64 {
        (self.opcode & 0xffff) as u64
    }
    fn rs64(&self) -> u64 {
        self.cpu.regs[self.rs()]
    }
    fn rt64(&self) -> u64 {
        self.cpu.regs[self.rt()]
    }
    fn rd64(&self) -> u64 {
        self.cpu.regs[self.rd()]
    }
    fn rs32(&self) -> u32 {
        self.rs64() as u32
    }
    fn rt32(&self) -> u32 {
        self.rt64() as u32
    }
    fn rd32(&self) -> u32 {
        self.rd64() as u32
    }
    fn irs32(&self) -> i32 {
        self.rs64() as i32
    }
    fn irt32(&self) -> i32 {
        self.rt64() as i32
    }
    fn ird32(&self) -> i32 {
        self.rd64() as i32
    }
    fn mrs64(&'a mut self) -> &'a mut u64 {
        &mut self.cpu.regs[self.rs()]
    }
    fn mrt64(&'a mut self) -> &'a mut u64 {
        &mut self.cpu.regs[self.rt()]
    }
    fn mrd64(&'a mut self) -> &'a mut u64 {
        &mut self.cpu.regs[self.rd()]
    }
    fn hex(&self) -> String {
        format!("{:x}", self.opcode)
    }
}

pub struct Cpu {
    regs: [u64; 32],
    hi: u64,
    lo: u64,

    cop0: Cop0,
    logger: slog::Logger,
    bus: Rc<RefCell<Box<Bus>>>,
    pc: u32,
    branch_pc: u32,
    clock: i64,
    until: i64,
    tight_exit: bool,
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
        };
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
                0x18 => {
                    // MULT
                    let (lo, hi) = i64::wrapping_mul(op.rt32().isx64(), op.rs32().isx64()).hi_lo();
                    op.cpu.lo = lo;
                    op.cpu.hi = hi;
                }
                0x19 => {
                    // MULTU
                    let (lo, hi) = u64::wrapping_mul(op.rt32() as u64, op.rs32() as u64).hi_lo();
                    op.cpu.lo = lo;
                    op.cpu.hi = hi;
                }
                0x20 => {
                    // ADD
                    *op.mrd64() = (op.rs32() as i32)
                        .checked_add(op.rt32() as i32)
                        .unwrap_or_else(|| {
                            op.cpu.trap_overflow();
                            (op.rs32() + op.rt32()) as i32
                        })
                        .sx64();
                }
                0x21 => *op.mrd64() = (op.rs32() + op.rt32()).sx64(), // ADDU
                0x22 => {
                    // SUB
                    *op.mrd64() = (op.rs32() as i32)
                        .checked_sub(op.rt32() as i32)
                        .unwrap_or_else(|| {
                            op.cpu.trap_overflow();
                            (op.rs32() - op.rt32()) as i32
                        })
                        .sx64();
                }
                0x23 => *op.mrd64() = (op.rs32() - op.rt32()).sx64(), // SUBU
                0x24 => *op.mrd64() = op.rs64() & op.rt64(),          // AND
                0x25 => *op.mrd64() = op.rs64() | op.rt64(),          // OR
                0x26 => *op.mrd64() = op.rs64() ^ op.rt64(),          // XOR
                0x27 => *op.mrd64() = !(op.rs64() | op.rt64()),       // NOR
                0x2A => *op.mrd64() = (op.irs32() < op.irt32()) as u64, // SLT
                0x2B => *op.mrd64() = (op.rs32() < op.rt32()) as u64, // SLTU
                _ => panic!("unimplemented special opcode: func={:x?}", op.special()),
            },

            0x0A => *op.mrd64() = (op.irs32() < op.sximm32()) as u64, // SLTI
            0x0B => *op.mrd64() = (op.rs32() < op.sximm32() as u32) as u64, // SLTIU
            0x0C => *op.mrd64() = op.rs64() & op.imm64(),             // ANDI
            0x0D => *op.mrd64() = op.rs64() | op.imm64(),             // ORI
            0x0E => *op.mrd64() = op.rs64() ^ op.imm64(),             // XORI
            0x0F => *op.mrt64() = (op.sximm32() << 16).sx64(),        // LUI
            0x10 => Cop0::op(op.cpu, opcode),                         // COP0
            0x11 => op.cpu.cop(1, opcode),                            // COP1
            0x12 => op.cpu.cop(2, opcode),                            // COP2
            0x13 => op.cpu.cop(3, opcode),                            // COP3
            0x14 => {
                // BEQL
                let (cond, tgt) = (op.rt64() == op.rs64(), op.btgt());
                op.cpu.branch_likely(cond, tgt);
            }
            0x23 => *op.mrt64() = op.cpu.read::<u32>(op.ea()).sx64(), // LW

            _ => panic!("unimplemented opcode: func={:x?}", op.op().hex()),
        }
    }

    fn fetch(&self, addr: u32) -> MemIoR<u32> {
        self.bus.borrow().fetch_read::<u32>(addr & 0x1FFF_FFFC)
    }

    fn read<U: MemInt>(&self, addr: u32) -> U {
        info!(self.logger, "read memory"; o!("size"=>U::SIZE, "addr" => (addr & 0x1FFF_FFFC).hex()));
        self.bus.borrow().read::<U>(addr & 0x1FFF_FFFC)
    }

    fn branch_likely(&mut self, cond: bool, tgt: u32) {
        if cond {
            self.branch_pc = tgt;
            self.tight_exit = true;
        } else {
            // branch not taken, skip delay slot
            self.pc += 4;
            self.clock += 1;
            self.tight_exit = true;
        }
    }

    pub fn run(&mut self, until: i64) {
        self.until = until;
        while self.clock < self.until {
            let mut iter = self.fetch(self.pc).iter().unwrap();

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
                let op = iter.next().unwrap_or_else(|| self.fetch(self.pc).read());
                self.pc = self.branch_pc;
                self.op(op);
            }
        }
    }
}
