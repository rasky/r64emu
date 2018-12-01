macro_rules! enum_str {
    (enum $name:ident {
        $($variant:ident = $val:expr),*,
    }) => {
        enum $name {
            $($variant = $val),*
        }

        impl $name {
            fn name(&self) -> &'static str {
                match self {
                    $($name::$variant => stringify!($variant)),*
                }
            }
        }
    };

    (enum $name:ident {
        $($variant:ident),*,
    }) => {
        enum $name {
            $($variant),*
        }

        impl $name {
            fn name(&self) -> &'static str {
                match self {
                    $($name::$variant => stringify!($variant)),*
                }
            }
        }
    };
}

#[derive(Copy, Clone, PartialEq)]
pub(crate) struct Reg(usize);

const REG_HI: Reg = Reg(32);
const REG_LO: Reg = Reg(33);

impl Reg {
    pub(crate) fn name(&self) -> &'static str {
        const NAMES: [&'static str; 34] = [
            "zr", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5",
            "t6", "t7", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1",
            "gp", "sp", "fp", "ra", "hi", "lo",
        ];
        NAMES[self.0]
    }
}

enum_str!{
    enum Insn {
        UNKNOWN,
        UNKSPECIAL,
        ADDIU, DADDI, DADDIU,
        ANDI, ORI, XORI,
        LUI,
        ADD, ADDU, SUB, SUBU, DADD, DADDU, DSUB, DSUBU,
        AND, OR, XOR, NOR, SLT, SLTU,
        J, JAL, JR, JALR,
        BEQ, BNE, BLEZ, BGTZ,
        BEQL, BNEL, BLEZL, BGTZL,
        LB, LBU, LH, LHU, LW, LWL, LWR, LWU,
        SB, SH, SW, SWL, SWR,
        SLL, SRL, SRA, SLLV, SRLV, SRAV, DSLLV, DSRLV, DSRAV,
        MULT, MULTU, DIV, DIVU, DMULT, DMULTU, DDIV, DDIVU,
        MFHI, MFLO, MTHI, MTLO,
        CACHE, SYNC, BREAK,

        // Fake insn
        NOP, LI, BEQZ, BNEZ,
    }
}

#[derive(Copy, Clone, PartialEq)]
enum Operand {
    Null,
    IReg(Reg),    // Input register
    OReg(Reg),    // Output register
    IOReg(Reg),   // Input/Output register
    Imm8(u8),     // 8-bit immediate
    Imm16(u16),   // 16-bit immediate
    Imm32(u32),   // 32-bit immediate
    Imm64(u64),   // 64-bit immediate
    HidIReg(Reg), // Implicit input register (not part of disasm)
    HidOReg(Reg), // Implicit output register (not part of disasm)
}

impl Operand {
    fn is_hidden(self) -> bool {
        use self::Operand::*;
        match self {
            HidIReg(_) | HidOReg(_) => true,
            _ => false,
        }
    }

    fn input(self) -> Option<Reg> {
        use self::Operand::*;
        match self {
            IReg(r) | IOReg(r) | HidIReg(r) => Some(r),
            _ => None,
        }
    }

    fn output(self) -> Option<Reg> {
        use self::Operand::*;
        match self {
            OReg(r) | IOReg(r) | HidOReg(r) => Some(r),
            _ => None,
        }
    }
}

pub(crate) struct DecodedInsn {
    op: Insn,
    operands: [Operand; 4],
}

impl DecodedInsn {
    fn new0(op: Insn) -> Self {
        DecodedInsn {
            op,
            operands: [Operand::Null, Operand::Null, Operand::Null, Operand::Null],
        }
    }
    fn new1(op: Insn, arg1: Operand) -> Self {
        DecodedInsn {
            op,
            operands: [arg1, Operand::Null, Operand::Null, Operand::Null],
        }
    }
    fn new2(op: Insn, arg1: Operand, arg2: Operand) -> Self {
        DecodedInsn {
            op,
            operands: [arg1, arg2, Operand::Null, Operand::Null],
        }
    }
    fn new3(op: Insn, arg1: Operand, arg2: Operand, arg3: Operand) -> Self {
        DecodedInsn {
            op,
            operands: [arg1, arg2, arg3, Operand::Null],
        }
    }
    fn new4(op: Insn, arg1: Operand, arg2: Operand, arg3: Operand, arg4: Operand) -> Self {
        DecodedInsn {
            op,
            operands: [arg1, arg2, arg3, arg4],
        }
    }

    pub(crate) fn disasm(&self) -> String {
        use self::Insn::*;
        use self::Operand::*;

        let mut dis = self.op.name().to_lowercase().to_owned();
        let args = self
            .operands
            .iter()
            .filter_map(|o| match o {
                Null => None,
                IReg(r) => Some(r.name().to_owned()),
                OReg(r) => Some(r.name().to_owned()),
                IOReg(r) => Some(r.name().to_owned()),
                Imm8(v) => Some(format!("{}", v)),
                Imm16(v) => Some(format!("0x{:x}", v)),
                Imm32(v) => Some(format!("0x{:x}", v)),
                Imm64(v) => Some(format!("0x{:x}", v)),
                HidIReg(r) => Some(r.name().to_owned()),
                HidOReg(r) => Some(r.name().to_owned()),
            })
            .collect::<Vec<String>>();

        dis += "\t";
        dis += &match self.op {
            LB | LBU | LH | LHU | LW | LWL | LWR | SB | SB | SH | SW | SWL | SWR => {
                format!("{},{}({})", args[0], args[2], args[1])
            }
            _ if args.len() == 4 => format!("{},{},{},{}", args[0], args[1], args[2], args[3]),
            _ if args.len() == 3 => format!("{},{},{}", args[0], args[1], args[2]),
            _ if args.len() == 2 => format!("{},{}", args[0], args[1]),
            _ if args.len() == 1 => format!("{}", args[0]),
            _ if args.len() == 0 => String::new(),
            _ => unimplemented!(),
        };
        dis
    }

    pub(crate) fn decode(opcode: u32, pc: u64) -> Self {
        use self::Insn::*;
        use self::Operand::*;

        let op = opcode >> 26;
        let special = opcode & 0x3f;
        let sa = ((opcode >> 6) & 0x1f) as u8;
        let rs = Reg(((opcode >> 21) & 0x1f) as usize);
        let rt = Reg(((opcode >> 16) & 0x1f) as usize);
        let rd = Reg(((opcode >> 11) & 0x1f) as usize);
        let imm16 = (opcode & 0xffff) as u16;
        let sximm32 = (opcode & 0xffff) as i16 as i32 as u32;
        let sximm64 = (opcode & 0xffff) as i16 as i64;
        let btgt = (pc + sximm64 as u64 * 4) as u32;
        let jtgt = ((pc & 0xFFFF_FFFF_F000_0000) + ((opcode as u64 & 0x03FF_FFFF) * 4)) as u32;

        match op {
            0x00 => match special {
                0x00 => Self::new3(SLL, OReg(rd), IReg(rt), Imm8(sa)),
                0x02 => Self::new3(SRL, OReg(rd), IReg(rt), Imm8(sa)),
                0x03 => Self::new3(SRA, OReg(rd), IReg(rt), Imm8(sa)),
                0x04 => Self::new3(SLLV, OReg(rd), IReg(rt), IReg(rs)),
                0x06 => Self::new3(SRLV, OReg(rd), IReg(rt), IReg(rs)),
                0x07 => Self::new3(SRAV, OReg(rd), IReg(rt), IReg(rs)),
                0x08 => Self::new1(JR, IReg(rs)),
                0x09 => Self::new1(JALR, IReg(rs)),
                0x0D => Self::new0(BREAK),
                0x0F => Self::new0(SYNC),

                0x10 => Self::new2(MFHI, OReg(rd), HidIReg(REG_HI)),
                0x11 => Self::new2(MTHI, HidOReg(REG_HI), IReg(rs)),
                0x12 => Self::new2(MFLO, OReg(rd), HidIReg(REG_LO)),
                0x13 => Self::new2(MTLO, HidOReg(REG_LO), IReg(rs)),
                0x14 => Self::new3(DSLLV, OReg(rd), IReg(rt), IReg(rs)),
                0x16 => Self::new3(DSRLV, OReg(rd), IReg(rt), IReg(rs)),
                0x17 => Self::new3(DSRAV, OReg(rd), IReg(rt), IReg(rs)),
                0x18 => Self::new4(MULT, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
                0x19 => Self::new4(MULTU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
                0x1A => Self::new4(DIV, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
                0x1B => Self::new4(DIVU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
                0x1C => Self::new4(DMULT, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
                0x1D => Self::new4(DMULTU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
                0x1E => Self::new4(DDIV, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),
                0x1F => Self::new4(DDIVU, HidOReg(REG_HI), HidOReg(REG_LO), IReg(rs), IReg(rt)),

                0x20 => Self::new3(ADD, OReg(rd), IReg(rs), IReg(rt)),
                0x21 => Self::new3(ADDU, OReg(rd), IReg(rs), IReg(rt)),
                0x22 => Self::new3(SUB, OReg(rd), IReg(rs), IReg(rt)),
                0x23 => Self::new3(SUBU, OReg(rd), IReg(rs), IReg(rt)),
                0x24 => Self::new3(AND, OReg(rd), IReg(rs), IReg(rt)),
                0x25 => Self::new3(OR, OReg(rd), IReg(rs), IReg(rt)),
                0x26 => Self::new3(XOR, OReg(rd), IReg(rs), IReg(rt)),
                0x27 => Self::new3(NOR, OReg(rd), IReg(rs), IReg(rt)),
                0x2A => Self::new3(SLT, OReg(rd), IReg(rs), IReg(rt)),
                0x2B => Self::new3(SLTU, OReg(rd), IReg(rs), IReg(rt)),
                0x2C => Self::new3(DADD, OReg(rd), IReg(rs), IReg(rt)),
                0x2D => Self::new3(DADDU, OReg(rd), IReg(rs), IReg(rt)),
                0x2E => Self::new3(DSUB, OReg(rd), IReg(rs), IReg(rt)),
                0x2F => Self::new3(DSUBU, OReg(rd), IReg(rs), IReg(rt)),

                _ => Self::new1(UNKSPECIAL, Imm32(special)),
            },
            0x02 => Self::new1(J, Imm32(jtgt)),
            0x03 => Self::new1(JAL, Imm32(jtgt)),
            0x04 => Self::new3(BEQ, IReg(rs), IReg(rt), Imm32(btgt)),
            0x05 => Self::new3(BNE, IReg(rs), IReg(rt), Imm32(btgt)),
            0x06 => Self::new2(BLEZ, IReg(rs), Imm32(btgt)),
            0x07 => Self::new2(BGTZ, IReg(rs), Imm32(btgt)),
            0x09 => Self::new3(ADDIU, OReg(rt), IReg(rs), Imm16(imm16)),
            0x0C => Self::new3(ANDI, OReg(rt), IReg(rs), Imm16(imm16)),
            0x0D => Self::new3(ORI, OReg(rt), IReg(rs), Imm16(imm16)),
            0x0E => Self::new3(XORI, OReg(rt), IReg(rs), Imm16(imm16)),
            0x0F => Self::new2(LUI, OReg(rt), Imm16(imm16)),

            0x14 => Self::new3(BEQL, IReg(rs), IReg(rt), Imm32(btgt)),
            0x15 => Self::new3(BNEL, IReg(rs), IReg(rt), Imm32(btgt)),
            0x16 => Self::new2(BLEZL, IReg(rs), Imm32(btgt)),
            0x17 => Self::new2(BGTZL, IReg(rs), Imm32(btgt)),
            0x18 => Self::new3(DADDI, OReg(rt), IReg(rs), Imm32(btgt)),
            0x19 => Self::new3(DADDIU, OReg(rt), IReg(rs), Imm32(btgt)),

            0x20 => Self::new3(LB, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x21 => Self::new3(LH, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x22 => Self::new3(LWL, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x23 => Self::new3(LW, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x24 => Self::new3(LBU, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x25 => Self::new3(LHU, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x26 => Self::new3(LWR, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x27 => Self::new3(LWU, OReg(rt), IReg(rs), Imm32(sximm32)),
            0x28 => Self::new3(SB, IReg(rt), IReg(rs), Imm32(sximm32)),
            0x29 => Self::new3(SH, IReg(rt), IReg(rs), Imm32(sximm32)),
            0x2A => Self::new3(SWL, IReg(rt), IReg(rs), Imm32(sximm32)),
            0x2B => Self::new3(SW, IReg(rt), IReg(rs), Imm32(sximm32)),
            0x2E => Self::new3(SWR, IReg(rt), IReg(rs), Imm32(sximm32)),
            0x2F => Self::new0(CACHE),
            _ => Self::new1(UNKNOWN, Imm32(op)),
        }
    }

    pub(crate) fn humanize(self) -> Self {
        use self::Insn::*;
        use self::Operand::*;

        let zr = Reg(0);
        let op0 = self.operands[0];
        let op1 = self.operands[1];
        let op2 = self.operands[2];
        let op3 = self.operands[3];
        match self.op {
            SLL if self.operands[0] == OReg(zr) && self.operands[1] == IReg(zr) => Self::new0(NOP),
            ADDIU | ORI if self.operands[1] == IReg(zr) => Self::new2(LI, op0, op2),
            BNE if self.operands[1] == IReg(zr) => Self::new2(BNEZ, op0, op2),
            BEQ if self.operands[1] == IReg(zr) => Self::new2(BEQZ, op0, op2),
            _ => self,
        }
    }
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
