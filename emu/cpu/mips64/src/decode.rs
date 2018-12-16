extern crate emu;
use super::cpu::Cpu;
use emu::dbg::Operand;

// Decoding format for arguments of load/store ops
const MEMOP_FMT: &'static str = "{},{}({})";

// Register names
pub const REG_NAMES: [&'static str; 34] = [
    "zr", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
    "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp", "fp", "ra",
    "hi", "lo",
];

/// DecodedInsn stores a MIPS64 decoded instruction.
///
/// We can't use enums here because there could be external
/// COP implementations (accessed through trait obejcts) and
/// we can't extend a enum.
pub type DecodedInsn = emu::dbg::DecodedInsn<&'static str, &'static str>;

macro_rules! decode_cop {
    ($cpu:ident, $opcode:ident, $pc:ident, $copn:ident, $default:expr) => {{
        match $cpu.$copn() {
            None => DecodedInsn::new0($default),
            Some(ref cop) => cop.decode($opcode, $pc),
        }
    }};
}

fn decode1(cpu: &Cpu, opcode: u32, pc: u64) -> DecodedInsn {
    use self::Operand::*;

    let op = opcode >> 26;
    let special = opcode & 0x3f;
    let sa = ((opcode >> 6) & 0x1f) as u8;
    let vrt = (opcode >> 16) & 0x1f;
    let rs = REG_NAMES[((opcode >> 21) & 0x1f) as usize].into();
    let rt = REG_NAMES[((opcode >> 16) & 0x1f) as usize].into();
    let rd = REG_NAMES[((opcode >> 11) & 0x1f) as usize].into();
    let imm16 = (opcode & 0xffff) as u16;
    let sximm32 = (opcode & 0xffff) as i16 as i32 as u32;
    let sximm64 = (opcode & 0xffff) as i16 as i64;
    let btgt = (pc + 4 + sximm64 as u64 * 4) as u32;
    let jtgt = (((pc + 4) & 0xFFFF_FFFF_F000_0000) + ((opcode as u64 & 0x03FF_FFFF) * 4)) as u32;
    let reghi = "hi".into();
    let reglo = "lo".into();

    match op {
        0x00 => match special {
            0x00 => DecodedInsn::new3("sll", OReg(rd), IReg(rt), Imm8(sa)),
            0x02 => DecodedInsn::new3("srl", OReg(rd), IReg(rt), Imm8(sa)),
            0x03 => DecodedInsn::new3("sra", OReg(rd), IReg(rt), Imm8(sa)),
            0x04 => DecodedInsn::new3("sllv", OReg(rd), IReg(rt), IReg(rs)),
            0x06 => DecodedInsn::new3("srlv", OReg(rd), IReg(rt), IReg(rs)),
            0x07 => DecodedInsn::new3("srav", OReg(rd), IReg(rt), IReg(rs)),
            0x08 => DecodedInsn::new1("jr", IReg(rs)),
            0x09 => DecodedInsn::new1("jalr", IReg(rs)),
            0x0D => DecodedInsn::new0("break"),
            0x0F => DecodedInsn::new0("sync"),

            0x10 => DecodedInsn::new2("mfhi", OReg(rd), HidIReg(reghi)),
            0x11 => DecodedInsn::new2("mthi", HidOReg(reghi), IReg(rs)),
            0x12 => DecodedInsn::new2("mflo", OReg(rd), HidIReg(reglo)),
            0x13 => DecodedInsn::new2("mtlo", HidOReg(reglo), IReg(rs)),
            0x14 => DecodedInsn::new3("dsllv", OReg(rd), IReg(rt), IReg(rs)),
            0x16 => DecodedInsn::new3("dsrlv", OReg(rd), IReg(rt), IReg(rs)),
            0x17 => DecodedInsn::new3("dsrav", OReg(rd), IReg(rt), IReg(rs)),
            0x18 => DecodedInsn::new4("mult", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),
            0x19 => DecodedInsn::new4("multu", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),
            0x1A => DecodedInsn::new4("div", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),
            0x1B => DecodedInsn::new4("divu", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),
            0x1C => DecodedInsn::new4("dmult", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),
            0x1D => DecodedInsn::new4("dmultu", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),
            0x1E => DecodedInsn::new4("ddiv", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),
            0x1F => DecodedInsn::new4("ddivu", HidOReg(reghi), HidOReg(reglo), IReg(rs), IReg(rt)),

            0x20 => DecodedInsn::new3("add", OReg(rd), IReg(rs), IReg(rt)),
            0x21 => DecodedInsn::new3("addu", OReg(rd), IReg(rs), IReg(rt)),
            0x22 => DecodedInsn::new3("sub", OReg(rd), IReg(rs), IReg(rt)),
            0x23 => DecodedInsn::new3("subu", OReg(rd), IReg(rs), IReg(rt)),
            0x24 => DecodedInsn::new3("and", OReg(rd), IReg(rs), IReg(rt)),
            0x25 => DecodedInsn::new3("or", OReg(rd), IReg(rs), IReg(rt)),
            0x26 => DecodedInsn::new3("xor", OReg(rd), IReg(rs), IReg(rt)),
            0x27 => DecodedInsn::new3("nor", OReg(rd), IReg(rs), IReg(rt)),
            0x2A => DecodedInsn::new3("slt", OReg(rd), IReg(rs), IReg(rt)),
            0x2B => DecodedInsn::new3("sltu", OReg(rd), IReg(rs), IReg(rt)),
            0x2C => DecodedInsn::new3("dadd", OReg(rd), IReg(rs), IReg(rt)),
            0x2D => DecodedInsn::new3("daddu", OReg(rd), IReg(rs), IReg(rt)),
            0x2E => DecodedInsn::new3("dsub", OReg(rd), IReg(rs), IReg(rt)),
            0x2F => DecodedInsn::new3("dsubu", OReg(rd), IReg(rs), IReg(rt)),

            _ => DecodedInsn::new1("unkspc", Imm32(special)),
        },
        0x01 => match vrt {
            0x00 => DecodedInsn::new2("bltz", IReg(rs), Target(btgt.into())),
            0x01 => DecodedInsn::new2("bgez", IReg(rs), Target(btgt.into())),
            0x02 => DecodedInsn::new2("bltzl", IReg(rs), Target(btgt.into())),
            0x03 => DecodedInsn::new2("bgezl", IReg(rs), Target(btgt.into())),
            0x10 => DecodedInsn::new2("bltzal", IReg(rs), Target(btgt.into())),
            0x11 => DecodedInsn::new2("bgezal", IReg(rs), Target(btgt.into())),
            0x12 => DecodedInsn::new2("bltzall", IReg(rs), Target(btgt.into())),
            0x13 => DecodedInsn::new2("bgezall", IReg(rs), Target(btgt.into())),

            _ => DecodedInsn::new1("unkrimm", Imm32(vrt)),
        },
        0x02 => DecodedInsn::new1("j", Target(jtgt.into())),
        0x03 => DecodedInsn::new1("jal", Target(jtgt.into())),
        0x04 => DecodedInsn::new3("beq", IReg(rs), IReg(rt), Target(btgt.into())),
        0x05 => DecodedInsn::new3("bne", IReg(rs), IReg(rt), Target(btgt.into())),
        0x06 => DecodedInsn::new2("blez", IReg(rs), Target(btgt.into())),
        0x07 => DecodedInsn::new2("bgtz", IReg(rs), Target(btgt.into())),
        0x08 => DecodedInsn::new3("addi", OReg(rt), IReg(rs), Imm16(imm16)),
        0x09 => DecodedInsn::new3("addiu", OReg(rt), IReg(rs), Imm16(imm16)),
        0x0A => DecodedInsn::new3("slti", OReg(rt), IReg(rs), Imm16(imm16)),
        0x0B => DecodedInsn::new3("sltiu", OReg(rt), IReg(rs), Imm16(imm16)),
        0x0C => DecodedInsn::new3("andi", OReg(rt), IReg(rs), Imm16(imm16)),
        0x0D => DecodedInsn::new3("ori", OReg(rt), IReg(rs), Imm16(imm16)),
        0x0E => DecodedInsn::new3("xori", OReg(rt), IReg(rs), Imm16(imm16)),
        0x0F => DecodedInsn::new2("lui", OReg(rt), Imm16(imm16)),

        0x10 => decode_cop!(cpu, opcode, pc, cop0, "cop0?"),
        0x11 => decode_cop!(cpu, opcode, pc, cop1, "cop1?"),
        0x12 => decode_cop!(cpu, opcode, pc, cop2, "cop2?"),
        0x13 => decode_cop!(cpu, opcode, pc, cop3, "cop3?"),
        0x14 => DecodedInsn::new3("beql", IReg(rs), IReg(rt), Target(btgt.into())),
        0x15 => DecodedInsn::new3("bnel", IReg(rs), IReg(rt), Target(btgt.into())),
        0x16 => DecodedInsn::new2("blezl", IReg(rs), Target(btgt.into())),
        0x17 => DecodedInsn::new2("bgtzl", IReg(rs), Target(btgt.into())),
        0x18 => DecodedInsn::new3("daddi", OReg(rt), IReg(rs), Target(btgt.into())),
        0x19 => DecodedInsn::new3("daddiu", OReg(rt), IReg(rs), Target(btgt.into())),

        0x20 => DecodedInsn::new3("lb", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x21 => DecodedInsn::new3("lh", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x22 => DecodedInsn::new3("lwl", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x23 => DecodedInsn::new3("lw", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x24 => DecodedInsn::new3("lbu", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x25 => DecodedInsn::new3("lhu", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x26 => DecodedInsn::new3("lwr", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x27 => DecodedInsn::new3("lwu", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x28 => DecodedInsn::new3("sb", IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x29 => DecodedInsn::new3("sh", IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2A => DecodedInsn::new3("swl", IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2B => DecodedInsn::new3("sw", IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2E => DecodedInsn::new3("swr", IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x2F => DecodedInsn::new0("cache"),

        0x31 => decode_cop!(cpu, opcode, pc, cop1, "lwc1?"),
        0x32 => decode_cop!(cpu, opcode, pc, cop2, "lwc2?"),
        0x35 => decode_cop!(cpu, opcode, pc, cop1, "ldc1?"),
        0x36 => decode_cop!(cpu, opcode, pc, cop2, "ldc2?"),
        0x37 => DecodedInsn::new3("ld", OReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),
        0x39 => decode_cop!(cpu, opcode, pc, cop1, "swc1?"),
        0x3A => decode_cop!(cpu, opcode, pc, cop2, "swc2?"),
        0x3D => decode_cop!(cpu, opcode, pc, cop1, "sdc1?"),
        0x3E => decode_cop!(cpu, opcode, pc, cop2, "sdc2?"),
        0x3F => DecodedInsn::new3("sd", IReg(rt), Imm32(sximm32), IReg(rs)).with_fmt(MEMOP_FMT),

        _ => DecodedInsn::new1("unknown", Imm32(op)),
    }
}

fn humanize(insn: DecodedInsn) -> DecodedInsn {
    use self::Operand::*;

    let zr = REG_NAMES[0].into();
    let op0 = insn.args[0];
    let op1 = insn.args[1];
    let op2 = insn.args[2];
    let _op3 = insn.args[3];
    match insn.op {
        "sll" if op0 == OReg(zr) && op1 == IReg(zr) => DecodedInsn::new0("nop"),
        "addi" | "addiu" | "ori" | "xori" if op1 == IReg(zr) => DecodedInsn::new2("li", op0, op2),
        "bne" if op1 == IReg(zr) => DecodedInsn::new2("bnez", op0, op2),
        "beq" if op0 == IReg(zr) && op1 == IReg(zr) => DecodedInsn::new1("j", op2), // relocatable encoding
        "beq" if op1 == IReg(zr) => DecodedInsn::new2("beqz", op0, op2),
        "bnel" if op0 == IReg(zr) => DecodedInsn::new2("bnezl", op1, op2),
        "bnel" if op1 == IReg(zr) => DecodedInsn::new2("bnezl", op0, op2),
        "beql" if op0 == IReg(zr) => DecodedInsn::new2("beqzl", op1, op2),
        "beql" if op1 == IReg(zr) => DecodedInsn::new2("beqzl", op0, op2),
        "bgez" if op0 == IReg(zr) => DecodedInsn::new1("j", op1), // relocatable encoding
        "bgezal" if op0 == IReg(zr) => DecodedInsn::new1("jal", op1), // relocatable encoding
        "or" if op1 == IReg(zr) && op2 == IReg(zr) => DecodedInsn::new2("li", op0, Imm32(0)),
        "or" if op1 == IReg(zr) => DecodedInsn::new2("move", op0, op2),
        "or" if op2 == IReg(zr) => DecodedInsn::new2("move", op0, op1),
        _ => insn,
    }
}

pub(crate) fn decode(cpu: &Cpu, opcode: u32, pc: u64) -> DecodedInsn {
    humanize(decode1(cpu, opcode, pc))
}
