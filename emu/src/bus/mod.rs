extern crate byteorder;

mod bus;
mod device;
mod mem;
mod memint;
mod radix;
mod regs;

pub use self::bus::Bus;
pub use self::mem::{Mem, MemFlags};
pub use self::regs::{Reg, RegFlags};

pub mod le {
    use super::byteorder::LittleEndian;
    pub use super::RegFlags;
    pub type Bus<'a> = super::Bus<'a, LittleEndian>;
    pub type Reg8 = super::Reg<LittleEndian, u8>;
    pub type Reg16 = super::Reg<LittleEndian, u16>;
    pub type Reg32 = super::Reg<LittleEndian, u32>;
    pub type Reg64 = super::Reg<LittleEndian, u64>;
}

pub mod be {
    use super::byteorder::BigEndian;
    pub use super::RegFlags;
    pub type Bus<'a> = super::Bus<'a, BigEndian>;
    pub type Reg8 = super::Reg<BigEndian, u8>;
    pub type Reg16 = super::Reg<BigEndian, u16>;
    pub type Reg32 = super::Reg<BigEndian, u32>;
    pub type Reg64 = super::Reg<BigEndian, u64>;
}
