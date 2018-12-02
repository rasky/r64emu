extern crate emu;
use emu::dbg::{DecodedInsn, Operand};
use std::fmt;

#[derive(Copy, Clone, PartialEq)]
pub struct DecodeReg(usize);

const REG_HI: DecodeReg = DecodeReg(32);
const REG_LO: DecodeReg = DecodeReg(33);

impl fmt::Display for DecodeReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const NAMES: [&'static str; 34] = [
            "zr", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5",
            "t6", "t7", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1",
            "gp", "sp", "fp", "ra", "hi", "lo",
        ];
        write!(f, "{}", NAMES[self.0])
    }
}

enum_str!{
    pub enum Insn {
        UNKNOWN, UNKSPC, UNKRIMM,
        ADDI, ADDIU, DADDI, DADDIU,
        ANDI, ORI, XORI,
        LUI,
        ADD, ADDU, SUB, SUBU, DADD, DADDU, DSUB, DSUBU,
        AND, OR, XOR, NOR, SLT, SLTU,
        J, JAL, JR, JALR,
        BEQ, BNE, BLEZ, BGTZ,
        BEQL, BNEL, BLEZL, BGTZL,
        BLTZ, BGEZ, BLTZAL, BGEZAL,
        BLTZL, BGEZL, BLTZALL, BGEZALL,
        LB, LBU, LH, LHU, LW, LWL, LWR, LWU,
        SB, SH, SW, SWL, SWR,
        SLL, SRL, SRA, SLLV, SRLV, SRAV, DSLLV, DSRLV, DSRAV,
        MULT, MULTU, DIV, DIVU, DMULT, DMULTU, DDIV, DDIVU,
        MFHI, MFLO, MTHI, MTLO,
        CACHE, SYNC, BREAK,

        // Fake insn
        NOP, LI, BEQZ, BNEZ, BEQZL, BNEZL, MOVE,
    }
}

// Decoding format for arguments of load/store ops
const MEMOP_FMT: &'static str = "{},{}({})";

fn decode1(opcode: u32, pc: u64) -> DecodedInsn<Insn, DecodeReg> {
    use self::Insn::*;
    use self::Operand::*;

    let op = opcode >> 26;
    let special = opcode & 0x3f;
    let sa = ((opcode >> 6) & 0x1f) as u8;
    let vrt = (opcode >> 16) & 0x1f;
    let rs = DecodeReg(((opcode >> 21) & 0x1f) as usize);
    let rt = DecodeReg(((opcode >> 16) & 0x1f) as usize);
    let rd = DecodeReg(((opcode >> 11) & 0x1f) as usize);
    let imm16 = (opcode & 0xffff) as u16;
    let sximm32 = (opcode & 0xffff) as i16 as i32 as u32;
    let sximm64 = (opcode & 0xffff) as i16 as i64;
    let btgt = (pc + sximm64 as u64 * 4) as u32;
    let jtgt = ((pc & 0xFFFF_FFFF_F000_0000) + ((opcode as u64 & 0x03FF_FFFF) * 4)) as u32;

    match op {
        0x00 => match special {
            0x00 => DecodedInsn::new3(SLL, OReg(rd), IReg(rt), Imm8(sa)),
            0x02 => DecodedInsn::new3(SRL, OReg(rd), IReg(rt), Imm8(sa)),
            0x03 => DecodedInsn::new3(SRA, OReg(rd), IReg(rt), Imm8(sa)),
            0x04 => DecodedInsn::new3(SLLV, OReg(rd), IReg(rt), IReg(rs)),
            0x06 => DecodedInsn::new3(SRLV, OReg(rd), IReg(rt), IReg(rs)),
            0x07 => DecodedInsn::new3(SRAV, OReg(rd), IReg(rt), IReg(rs)),
            0x08 => DecodedInsn::new1(JR, IReg(rs)),
            0x09 => DecodedInsn::new1(JALR, IReg(rs)),
            0x0D => DecodedInsn::new0(BREAK),
            0x0F => DecodedInsn::new0(SYNC),

            0x10 => DecodedInsn::new2(MFHI, OReg(rd), HidIReg(REG_HI)),
            0x11 => DecodedInsn::new2(MTHI, HidOReg(REG_HI), IReg(rs)),
            0x12 => DecodedInsn::new2(MFLO, OReg(rd), HidIReg(REG_LO)),
            0x13 => DecodedInsn::new2(MTLO, HidOReg(REG_LO), IReg(rs)),
            0x14 => DecodedInsn::new3(DSLLV, OReg(rd), IReg(rt), IReg(rs)),
            0x16 => DecodedInsn::new3(DSRLV, OReg(rd), IReg(rt), IReg(rs)),
            0x17 => DecodedInsn::new3(DSRAV, OReg(rd), IReg(rt), IReg(rs)),
            0x18 => DecodedInsn::new4(MULT, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
            0x19 => DecodedInsn::new4(MULTU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
            0x1A => DecodedInsn::new4(DIV, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
            0x1B => DecodedInsn::new4(DIVU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
            0x1C => DecodedInsn::new4(DMULT, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
            0x1D => DecodedInsn::new4(DMULTU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
            0x1E => DecodedInsn::new4(DDIV, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
            0x1F => DecodedInsn::new4(DDIVU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),

            0x20 => DecodedInsn::new3(ADD, OReg(rd), IReg(rs), IReg(rt)),
            0x21 => DecodedInsn::new3(ADDU, OReg(rd), IReg(rs), IReg(rt)),
            0x22 => DecodedInsn::new3(SUB, OReg(rd), IReg(rs), IReg(rt)),
            0x23 => DecodedInsn::new3(SUBU, OReg(rd), IReg(rs), IReg(rt)),
            0x24 => DecodedInsn::new3(AND, OReg(rd), IReg(rs), IReg(rt)),
            0x25 => DecodedInsn::new3(OR, OReg(rd), IReg(rs), IReg(rt)),
            0x26 => DecodedInsn::new3(XOR, OReg(rd), IReg(rs), IReg(rt)),
            0x27 => DecodedInsn::new3(NOR, OReg(rd), IReg(rs), IReg(rt)),
            0x2A => DecodedInsn::new3(SLT, OReg(rd), IReg(rs), IReg(rt)),
            0x2B => DecodedInsn::new3(SLTU, OReg(rd), IReg(rs), IReg(rt)),
            0x2C => DecodedInsn::new3(DADD, OReg(rd), IReg(rs), IReg(rt)),
            0x2D => DecodedInsn::new3(DADDU, OReg(rd), IReg(rs), IReg(rt)),
            0x2E => DecodedInsn::new3(DSUB, OReg(rd), IReg(rs), IReg(rt)),
            0x2F => DecodedInsn::new3(DSUBU, OReg(rd), IReg(rs), IReg(rt)),

            _ => DecodedInsn::new1(UNKSPC, Imm32(special)),
        },
        0x01 => match vrt {
            0x00 => DecodedInsn::new2(BLTZ, IReg(rs), Imm32(btgt)),
            0x01 => DecodedInsn::new2(BGEZ, IReg(rs), Imm32(btgt)),
            0x02 => DecodedInsn::new2(BLTZL, IReg(rs), Imm32(btgt)),
            0x03 => DecodedInsn::new2(BGEZL, IReg(rs), Imm32(btgt)),
            0x10 => DecodedInsn::new2(BLTZAL, IReg(rs), Imm32(btgt)),
            0x11 => DecodedInsn::new2(BGEZAL, IReg(rs), Imm32(btgt)),
            0x12 => DecodedInsn::new2(BLTZALL, IReg(rs), Imm32(btgt)),
            0x13 => DecodedInsn::new2(BGEZALL, IReg(rs), Imm32(btgt)),

            _ => DecodedInsn::new1(UNKRIMM, Imm32(vrt)),
        },
        0x02 => DecodedInsn::new1(J, Imm32(jtgt)),
        0x03 => DecodedInsn::new1(JAL, Imm32(jtgt)),
        0x04 => DecodedInsn::new3(BEQ, IReg(rs), IReg(rt), Imm32(btgt)),
        0x05 => DecodedInsn::new3(BNE, IReg(rs), IReg(rt), Imm32(btgt)),
        0x06 => DecodedInsn::new2(BLEZ, IReg(rs), Imm32(btgt)),
        0x07 => DecodedInsn::new2(BGTZ, IReg(rs), Imm32(btgt)),
        0x08 => DecodedInsn::new3(ADDI, OReg(rt), IReg(rs), Imm16(imm16)),
        0x09 => DecodedInsn::new3(ADDIU, OReg(rt), IReg(rs), Imm16(imm16)),
        0x0C => DecodedInsn::new3(ANDI, OReg(rt), IReg(rs), Imm16(imm16)),
        0x0D => DecodedInsn::new3(ORI, OReg(rt), IReg(rs), Imm16(imm16)),
        0x0E => DecodedInsn::new3(XORI, OReg(rt), IReg(rs), Imm16(imm16)),
        0x0F => DecodedInsn::new2(LUI, OReg(rt), Imm16(imm16)),

        0x14 => DecodedInsn::new3(BEQL, IReg(rs), IReg(rt), Imm32(btgt)),
        0x15 => DecodedInsn::new3(BNEL, IReg(rs), IReg(rt), Imm32(btgt)),
        0x16 => DecodedInsn::new2(BLEZL, IReg(rs), Imm32(btgt)),
        0x17 => DecodedInsn::new2(BGTZL, IReg(rs), Imm32(btgt)),
        0x18 => DecodedInsn::new3(DADDI, OReg(rt), IReg(rs), Imm32(btgt)),
        0x19 => DecodedInsn::new3(DADDIU, OReg(rt), IReg(rs), Imm32(btgt)),

        0x20 => DecodedInsn::new3(LB, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x21 => DecodedInsn::new3(LH, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x22 => DecodedInsn::new3(LWL, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x23 => DecodedInsn::new3(LW, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x24 => DecodedInsn::new3(LBU, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x25 => DecodedInsn::new3(LHU, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x26 => DecodedInsn::new3(LWR, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x27 => DecodedInsn::new3(LWU, OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x28 => DecodedInsn::new3(SB, IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x29 => DecodedInsn::new3(SH, IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2A => DecodedInsn::new3(SWL, IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2B => DecodedInsn::new3(SW, IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2E => DecodedInsn::new3(SWR, IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2F => DecodedInsn::new0(CACHE),
        _ => DecodedInsn::new1(UNKNOWN, Imm32(op)),
    }
}

fn humanize(insn: DecodedInsn<Insn, DecodeReg>) -> DecodedInsn<Insn, DecodeReg> {
    use self::Insn::*;
    use self::Operand::*;

    let zr = DecodeReg(0);
    let op0 = insn.args[0];
    let op1 = insn.args[1];
    let op2 = insn.args[2];
    let op3 = insn.args[3];
    match insn.op {
        SLL if op0 == OReg(zr) && op1 == IReg(zr) => DecodedInsn::new0(NOP),
        ADDI | ADDIU | ORI if op1 == IReg(zr) => DecodedInsn::new2(LI, op0, op2),
        BNE if op1 == IReg(zr) => DecodedInsn::new2(BNEZ, op0, op2),
        BEQ if op1 == IReg(zr) => DecodedInsn::new2(BEQZ, op0, op2),
        BNEL if op1 == IReg(zr) => DecodedInsn::new2(BNEZL, op0, op2),
        BEQL if op1 == IReg(zr) => DecodedInsn::new2(BEQZL, op0, op2),
        BEQZ | BGEZ if op0 == IReg(zr) => DecodedInsn::new1(J, op1), // relocatable encoding
        BGEZAL if op0 == IReg(zr) => DecodedInsn::new1(JAL, op1),    // relocatable encoding
        OR if op1 == IReg(zr) && op2 == IReg(zr) => DecodedInsn::new2(LI, op0, Imm32(0)),
        OR if op1 == IReg(zr) => DecodedInsn::new2(MOVE, op0, op2),
        OR if op2 == IReg(zr) => DecodedInsn::new2(MOVE, op0, op1),
        _ => insn,
    }
}

pub(crate) fn decode(opcode: u32, pc: u64) -> DecodedInsn<Insn, DecodeReg> {
    humanize(decode1(opcode, pc))
}

#[cfg(test)]
mod tests {
    use super::DecodedInsn;

    fn dis(op: u32, pc: u64) -> String {
        DecodedInsn::decode(op, pc).disasm()
    }
    fn hdis(op: u32, pc: u64) -> String {
        DecodedInsn::decode(op, pc).humanize().disasm()
    }

    #[test]
    fn disasm() {
        assert_eq!(dis(0x24040386, 0), "addiu\ta0,zr,0x386");
    }

    #[test]
    fn humanize() {
        assert_eq!(hdis(0x24040386, 0), "li\ta0,0x386");
    }

}
