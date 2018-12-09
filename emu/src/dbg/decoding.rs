/// Helper classes to write a CPU decoder / disassembler.
use runtime_fmt::rt_format_args;

use std::fmt;

const MAX_OPERANDS_PER_INSN: usize = 4;

/// enum_str allows to define a enum that implements Display
/// It can be useful while defining an Insn enum to be used
/// as generic argument to DecodedInsn.
#[macro_export]
macro_rules! enum_str {
    (pub enum $name:ident {
        $($variant:ident = $val:expr),*,
    }) => {
        #[derive(Copy, Clone, PartialEq)]
        pub enum $name {
            $($variant = $val),*
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                match self {
                    $($name::$variant => write!(f, "{}", stringify!($variant).to_lowercase())),*
                }
            }
        }
    };

    (pub enum $name:ident {
        $($variant:ident),*,
    }) => {
        #[derive(Copy, Clone, PartialEq)]
        pub enum $name {
            $($variant),*
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                match self {
                    $($name::$variant => write!(f, "{}", stringify!($variant).to_lowercase())),*
                }
            }
        }
    };
}

/// An instruction operand.
/// Reg is a type that can be used to identify a CPU register.
/// It does not store the runtime value of the register, but just
/// identifies the register itself, and implements fmt::Display for
/// the purpose of being displayed. It is usually either an
/// enum (possibly using enum_str!) or a string.

#[derive(Copy, Clone, PartialEq)]
pub enum Operand<Reg: fmt::Display + Copy + Clone + PartialEq> {
    Null,         // Unused operand slot
    IReg(Reg),    // Input register
    OReg(Reg),    // Output register
    IOReg(Reg),   // Input/Output register
    HidIReg(Reg), // Implicit input register (not part of disasm)
    HidOReg(Reg), // Implicit output register (not part of disasm)
    Imm8(u8),     // 8-bit immediate
    Imm16(u16),   // 16-bit immediate
    Imm32(u32),   // 32-bit immediate
    Imm64(u64),   // 64-bit immediate
}

impl<Reg> fmt::Display for Operand<Reg>
where
    Reg: fmt::Display + Copy + Clone + PartialEq,
{
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
            HidIReg(r) => write!(f, "{}", r),
            HidOReg(r) => write!(f, "{}", r),
        }
    }
}

impl<Reg> Operand<Reg>
where
    Reg: fmt::Display + Copy + Clone + PartialEq,
{
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

/// A decoded instruction. Insn is usually a string or a enum that selects
/// among all possible instructions, but any type would work
/// as long as it's value-like (Copy+Clone+PartialEq) and implements
/// fmt::Display for the purpose of being displayed in the disassembly window.
///
/// fmt is the formatting pattern (in std::fmt format) used to represent the arguments.
/// If None, the arguments will be displayed as comma-separated.
#[derive(Clone, PartialEq)]
pub struct DecodedInsn<Insn, Reg>
where
    Insn: fmt::Display + Copy + Clone + PartialEq,
    Reg: fmt::Display + Copy + Clone + PartialEq,
{
    pub op: Insn,
    pub fmt: Option<String>,
    pub args: [Operand<Reg>; MAX_OPERANDS_PER_INSN],
}

impl<Insn, Reg> DecodedInsn<Insn, Reg>
where
    Insn: fmt::Display + Copy + Clone + PartialEq,
    Reg: fmt::Display + Copy + Clone + PartialEq,
{
    pub fn new0<I: Into<Insn>>(op: I) -> Self {
        DecodedInsn {
            op: op.into(),
            fmt: None,
            args: [Operand::Null, Operand::Null, Operand::Null, Operand::Null],
        }
    }
    pub fn new1<I: Into<Insn>>(op: I, arg1: Operand<Reg>) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op
    }
    pub fn new2<I: Into<Insn>>(op: I, arg1: Operand<Reg>, arg2: Operand<Reg>) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op
    }
    pub fn new3<I: Into<Insn>>(
        op: I,
        arg1: Operand<Reg>,
        arg2: Operand<Reg>,
        arg3: Operand<Reg>,
    ) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op
    }
    pub fn new4<I: Into<Insn>>(
        op: I,
        arg1: Operand<Reg>,
        arg2: Operand<Reg>,
        arg3: Operand<Reg>,
        arg4: Operand<Reg>,
    ) -> Self {
        let mut op = Self::new0(op);
        op.args[0] = arg1;
        op.args[1] = arg2;
        op.args[2] = arg3;
        op.args[3] = arg4;
        op
    }

    pub fn with_fmt(mut self: Self, fmt: &str) -> Self {
        self.fmt = Some(fmt.to_owned());
        self
    }

    pub fn args(&self) -> impl Iterator<Item = &Operand<Reg>> {
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
