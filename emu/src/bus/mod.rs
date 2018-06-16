extern crate byteorder;

mod bus;
mod memint;
mod radix;
mod regs;

pub use self::bus::Bus;
pub use self::regs::{Reg, RegFlags};

pub mod le {
    use super::byteorder::LittleEndian;
    pub use super::RegFlags;
    pub type Bus<'a> = super::Bus<'a, LittleEndian>;
    pub type Reg8<'a> = super::Reg<'a, LittleEndian, u8>;
    pub type Reg16<'a> = super::Reg<'a, LittleEndian, u16>;
    pub type Reg32<'a> = super::Reg<'a, LittleEndian, u32>;
    pub type Reg64<'a> = super::Reg<'a, LittleEndian, u64>;
}

pub mod be {
    use super::byteorder::BigEndian;
    pub use super::RegFlags;
    pub type Bus<'a> = super::Bus<'a, BigEndian>;
    pub type Reg8<'a> = super::Reg<'a, BigEndian, u8>;
    pub type Reg16<'a> = super::Reg<'a, BigEndian, u16>;
    pub type Reg32<'a> = super::Reg<'a, BigEndian, u32>;
    pub type Reg64<'a> = super::Reg<'a, BigEndian, u64>;
}
