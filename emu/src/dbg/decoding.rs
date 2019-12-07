/// Helper classes to write a CPU decoder / disassembler.
use runtime_fmt::rt_format_args;

use std::fmt;

const MAX_OPERANDS_PER_INSN: usize = 8;

/// An instruction operand.
/// Reg is a type that can be used to identify a CPU register.
/// It does not store the runtime value of the register, but just
/// identifies the register itself, and implements fmt::Display for
/// the purpose of being displayed. It is usually either an
/// enum (possibly using enum_str!) or a string.

#[derive(Copy, Clone, PartialEq)]
pub enum Operand {
    Null,                  // Unused operand slot
    IReg(&'static str),    // Input register
    OReg(&'static str),    // Output register
    IOReg(&'static str),   // Input/Output register
    HidIReg(&'static str), // Implicit input register (not part of disasm)
    HidOReg(&'static str), // Implicit output register (not part of disasm)
    Imm8(u8),              // 8-bit immediate
    Imm16(u16),            // 16-bit immediate
    Imm32(u32),            // 32-bit immediate
    Imm64(u64),            // 64-bit immediate
    Target(u64),           // A branch target (absolute address)
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Operand::*;
        match self {
            Null => write!(f, "<NULL>"),
            IReg(r) => write!(f, "{}", r),
            OReg(r) => write!(f, "{}", r),
            IOReg(r) => write!(f, "{}", r),
            Imm8(v) => write!(f, "{}", v),
            Imm16(v) => write!(f, "0x{:x}", v),
            Imm32(v) => write!(f, "0x{:x}", v),
            Imm64(v) => write!(f, "0x{:x}", v),
            Target(v) => write!(f, "0x{:x}", v),
            HidIReg(r) => write!(f, "{}", r),
            HidOReg(r) => write!(f, "{}", r),
        }
    }
}

impl Operand {
    pub fn is_hidden(self) -> bool {
        use self::Operand::*;
        match self {
            HidIReg(_) | HidOReg(_) => true,
            _ => false,
        }
    }

    pub fn input(self) -> Option<&'static str> {
        use self::Operand::*;
        match self {
            IReg(r) | IOReg(r) | HidIReg(r) => Some(r),
            _ => None,
        }
    }

    pub fn output(self) -> Option<&'static str> {
        use self::Operand::*;
        match self {
            OReg(r) | IOReg(r) | HidOReg(r) => Some(r),
            _ => None,
        }
    }
}

/// A decoded instruction, composed of an opcode and zero to several arguments.
/// Both opcodes and operands are forced to be static strings, as it's a reasonable
/// constraint for disassemblers. It would be useless to genericize this structure
/// over an enum of opssible opcodes as the only usage is within the context of
/// disaplying a disassembled views, so using strings do not constraint the debugger
/// implementation in any way.
///
/// fmt is the formatting pattern (in std::fmt format) used to represent the arguments.
/// If None, the arguments will be displayed as comma-separated.
#[derive(Clone, PartialEq)]
pub struct DecodedInsn {
    pub op: &'static str,
    pub fmt: Option<String>,
    pub args: [Operand; MAX_OPERANDS_PER_INSN],
}

impl DecodedInsn {
    pub fn new0<I: Into<&'static str>>(op: I) -> Self {
        DecodedInsn {
            op: op.into(),
            fmt: None,
            args: [
                Operand::Null,
                Operand::Null,
                Operand::Null,
                Operand::Null,
                Operand::Null,
                Operand::Null,
                Operand::Null,
                Operand::Null,
            ],
        }
    }
    pub fn new1<I: Into<&'static str>>(op: I, arg1: Operand) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op
    }
    pub fn new2<I: Into<&'static str>>(op: I, arg1: Operand, arg2: Operand) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op
    }
    pub fn new3<I: Into<&'static str>>(op: I, arg1: Operand, arg2: Operand, arg3: Operand) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op
    }
    pub fn new4<I: Into<&'static str>>(
        op: I,
        arg1: Operand,
        arg2: Operand,
        arg3: Operand,
        arg4: Operand,
    ) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op.args[3] = arg4;
        op
    }
    pub fn new5<I: Into<&'static str>>(
        op: I,
        arg1: Operand,
        arg2: Operand,
        arg3: Operand,
        arg4: Operand,
        arg5: Operand,
    ) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op.args[3] = arg4;
        op.args[4] = arg5;
        op
    }
    pub fn new6<I: Into<&'static str>>(
        op: I,
        arg1: Operand,
        arg2: Operand,
        arg3: Operand,
        arg4: Operand,
        arg5: Operand,
        arg6: Operand,
    ) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op.args[3] = arg4;
        op.args[4] = arg5;
        op.args[5] = arg6;
        op
    }
    pub fn new7<I: Into<&'static str>>(
        op: I,
        arg1: Operand,
        arg2: Operand,
        arg3: Operand,
        arg4: Operand,
        arg5: Operand,
        arg6: Operand,
        arg7: Operand,
    ) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op.args[3] = arg4;
        op.args[4] = arg5;
        op.args[5] = arg6;
        op.args[6] = arg7;
        op
    }
    pub fn new8<I: Into<&'static str>>(
        op: I,
        arg1: Operand,
        arg2: Operand,
        arg3: Operand,
        arg4: Operand,
        arg5: Operand,
        arg6: Operand,
        arg7: Operand,
        arg8: Operand,
    ) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op.args[3] = arg4;
        op.args[4] = arg5;
        op.args[5] = arg6;
        op.args[6] = arg7;
        op.args[7] = arg8;
        op
    }

    pub fn with_fmt(mut self: Self, fmt: &str) -> Self {
        self.fmt = Some(fmt.to_owned());
        self
    }

    pub fn args(&self) -> impl Iterator<Item = &Operand> {
        self.args.iter().take_while(|o| *o != &Operand::Null)
    }

    // Return a string representation of the insn, which represents
    // the disassembled instruction.
    pub fn disasm(&self) -> String {
        // Get all args which are not hidden
        let args = self.args().filter(|o| !o.is_hidden()).collect::<Vec<_>>();

        if let Some(ref f) = self.fmt {
            // Custom formatting strings. Use rt_format
            match args.len() {
                4 => rt_format_args!(f, args[0], args[1], args[2], args[3])
                    .unwrap_or(rt_format_args!("<INVALID ARGUMENTS>").unwrap())
                    .with(|args| format!("{}\t{}", self.op, args)),
                3 => rt_format_args!(f, args[0], args[1], args[2])
                    .unwrap_or(rt_format_args!("<INVALID ARGUMENTS>").unwrap())
                    .with(|args| format!("{}\t{}", self.op, args)),
                2 => rt_format_args!(f, args[0], args[1])
                    .unwrap_or(rt_format_args!("<INVALID ARGUMENTS>").unwrap())
                    .with(|args| format!("{}\t{}", self.op, args)),
                1 => rt_format_args!(f, args[0])
                    .unwrap_or(rt_format_args!("<INVALID ARGUMENTS>").unwrap())
                    .with(|args| format!("{}\t{}", self.op, args)),
                0 => rt_format_args!(f)
                    .unwrap_or(rt_format_args!("<INVALID ARGUMENTS>").unwrap())
                    .with(|args| format!("{}\t{}", self.op, args)),
                _ => unreachable!(),
            }
        } else {
            // Standard formatting with commas. Use compile-time formatting.
            match args.len() {
                4 => format!(
                    "{}\t{},{},{},{}",
                    self.op, args[0], args[1], args[2], args[3]
                ),
                3 => format!("{}\t{},{},{}", self.op, args[0], args[1], args[2]),
                2 => format!("{}\t{},{}", self.op, args[0], args[1]),
                1 => format!("{}\t{}", self.op, args[0]),
                0 => format!("{}\t", self.op),
                _ => unreachable!(),
            }
        }
    }
}
