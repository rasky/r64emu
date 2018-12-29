use super::Arch;

pub struct ArchIII {}
pub struct ArchII {}
pub struct ArchI {}

impl Arch for ArchIII {
    #[inline(always)]
    fn has_op(_op: &'static str) -> bool {
        true
    }
}

impl Arch for ArchII {
    #[inline(always)]
    fn has_op(op: &'static str) -> bool {
        match op {
            // ArchIII-only instructions not available in ArchII
            "daddi" => false,
            "daddiu" => false,
            "dadd" => false,
            "daddu" => false,
            "dsub" => false,
            "dsubu" => false,
            "dmult" => false,
            "dmultu" => false,
            "ddiv" => false,
            "ddivu" => false,
            "dsll" => false,
            "dsrl" => false,
            "dsra" => false,
            "dsllv" => false,
            "dsrlv" => false,
            "dsrav" => false,
            "dsll32" => false,
            "dsrl32" => false,
            "dsra32" => false,
            "ld" => false,
            "ldc1" => false,
            "ldc2" => false,
            "sd" => false,
            "sdc1" => false,
            "sdc2" => false,
            _ => true,
        }
    }
}

impl Arch for ArchI {
    #[inline(always)]
    fn has_op(op: &'static str) -> bool {
        if !ArchII::has_op(op) {
            return false;
        }
        // ArchII-only instructions not available in ArchI
        match op {
            "beql" => false,
            "bnel" => false,
            "bgtzl" => false,
            "bgezl" => false,
            "btlzl" => false,
            "blezl" => false,
            "btlzall" => false,
            "bgezall" => false,
            _ => true,
        }
    }
}
