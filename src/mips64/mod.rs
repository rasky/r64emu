extern crate num;

mod cop0;
mod cpu;

pub use self::cpu::Cpu;

pub(crate) struct Mipsop<'a> {
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
    fn sa(&self) -> usize {
        ((self.opcode >> 6) & 0x1f) as usize
    }
    fn btgt(&self) -> u32 {
        self.cpu.pc + self.sximm32() as u32 * 4
    }
    fn jtgt(&self) -> u32 {
        (self.cpu.pc & 0xF000_0000) + ((self.opcode & 0x03FF_FFFF) * 4)
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
