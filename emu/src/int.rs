pub trait Numerics: Sized {
    type Unsigned: Numerics;

    fn isx32(self) -> i32;
    fn sx32(self) -> u32 {
        self.isx32() as u32
    }
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
    fn isx32(self) -> i32 {
        self as i8 as i32
    }
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
    fn isx32(self) -> i32 {
        self as i16 as i32
    }
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
    fn isx32(self) -> i32 {
        self
    }
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
    fn isx32(self) -> i32 {
        self as i32
    }
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
    fn isx32(self) -> i32 {
        panic!("isx32 for i64")
    }
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
    fn isx32(self) -> i32 {
        panic!("isx32 for u64")
    }
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

impl Numerics for i128 {
    type Unsigned = u128;

    #[inline(always)]
    fn isx32(self) -> i32 {
        panic!("isx32 for i128")
    }
    #[inline(always)]
    fn isx64(self) -> i64 {
        panic!("i128 isx64")
    }
    #[inline(always)]
    fn hi_lo(self) -> (u128, u128) {
        (self as u128).hi_lo()
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:016x}", self)
    }
}

impl Numerics for u128 {
    type Unsigned = u128;

    #[inline(always)]
    fn isx32(self) -> i32 {
        panic!("isx32 for u128")
    }
    #[inline(always)]
    fn isx64(self) -> i64 {
        panic!("u128 isx64")
    }
    #[inline(always)]
    fn hi_lo(self) -> (u128, u128) {
        (self >> 64, self & 0xffffffff_ffffffff)
    }
    #[inline(always)]
    fn hex(self) -> String {
        format!("0x{:016x}", self)
    }
}

pub struct HexSlice<'a>(&'a [u8]);

impl<'a> std::fmt::LowerHex for HexSlice<'a> {
    fn fmt(&self, fmtr: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        for byte in self.0 {
            fmtr.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
    }
}
