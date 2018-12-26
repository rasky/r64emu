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
            "daddi" => false,
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
        return true;
    }
}
