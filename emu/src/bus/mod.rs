extern crate byteorder;

mod bus;
mod device;
mod mem;
mod radix;
mod regs;

pub use self::bus::{Bus, BusFill, MemIoR, MemIoRIterator, MemIoW};
pub use self::device::{CurrentDeviceMap, Device, DeviceMap};
pub use self::mem::{Mem, MemFlags};
pub use self::regs::{Reg, RegDeref, RegFlags, RegRef};

pub mod le {
    use super::byteorder::LittleEndian;
    pub use super::{BusFill, Device, Mem, MemFlags, RegDeref, RegFlags};
    pub type Bus = super::Bus<LittleEndian>;
    pub type Reg8 = super::Reg<LittleEndian, u8>;
    pub type Reg16 = super::Reg<LittleEndian, u16>;
    pub type Reg32 = super::Reg<LittleEndian, u32>;
    pub type Reg64 = super::Reg<LittleEndian, u64>;
    pub type RegRef<U> = super::RegRef<LittleEndian, U>;
    pub type MemIoR<U> = super::MemIoR<LittleEndian, U>;
    pub type MemIoW<U> = super::MemIoW<LittleEndian, U>;
}

pub mod be {
    use super::byteorder::BigEndian;
    pub use super::{BusFill, Device, Mem, MemFlags, RegDeref, RegFlags};
    pub type Bus = super::Bus<BigEndian>;
    pub type Reg8 = super::Reg<BigEndian, u8>;
    pub type Reg16 = super::Reg<BigEndian, u16>;
    pub type Reg32 = super::Reg<BigEndian, u32>;
    pub type Reg64 = super::Reg<BigEndian, u64>;
    pub type RegRef<U> = super::RegRef<BigEndian, U>;
    pub type MemIoR<U> = super::MemIoR<BigEndian, U>;
    pub type MemIoW<U> = super::MemIoW<BigEndian, U>;
}
