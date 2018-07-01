pub trait Numerics: Sized {
    type Unsigned: Numerics;

    fn isx64(self) -> i64;
    fn sx64(self) -> u64 {
        self.isx64() as u64
    }
    fn hex(self) -> String;
    fn hi_lo(self) -> (Self::Unsigned, Self::Unsigned);
}

impl Numerics for u8 {
    type Unsigned = u8;

    #[inline(always)]
    fn isx64(self) -> i64 {
        self as i8 as i64
    }
    #[inline(always)]
    fn hi_lo(self) -> (u8, u8) {
        (self >> 4, self & 0xf)
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:02x}", self)
    }
}

impl Numerics for u16 {
    type Unsigned = u16;

    #[inline(always)]
    fn isx64(self) -> i64 {
        self as i16 as i64
    }
    #[inline(always)]
    fn hi_lo(self) -> (u16, u16) {
        (self >> 8, self & 0xff)
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:04x}", self)
    }
}

impl Numerics for i32 {
    type Unsigned = u32;

    #[inline(always)]
    fn isx64(self) -> i64 {
        self as i64
    }
    #[inline(always)]
    fn hi_lo(self) -> (u32, u32) {
        (self as u32).hi_lo()
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:08x}", self)
    }
}

impl Numerics for u32 {
    type Unsigned = u32;

    #[inline(always)]
    fn isx64(self) -> i64 {
        self as i32 as i64
    }
    #[inline(always)]
    fn hi_lo(self) -> (u32, u32) {
        (self >> 16, self & 0xfffff)
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:08x}", self)
    }
}

impl Numerics for i64 {
    type Unsigned = u64;

    #[inline(always)]
    fn isx64(self) -> i64 {
        self
    }
    #[inline(always)]
    fn hi_lo(self) -> (u64, u64) {
        (self as u64).hi_lo()
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:016x}", self)
    }
}

impl Numerics for u64 {
    type Unsigned = u64;

    #[inline(always)]
    fn isx64(self) -> i64 {
        self as i64
    }
    #[inline(always)]
    fn hi_lo(self) -> (u64, u64) {
        (self >> 32, self & 0xffffffff)
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:016x}", self)
    }
}
