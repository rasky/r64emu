extern crate byteorder;
extern crate num;

use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use self::num::PrimInt;

#[derive(Debug, Enum, Copy, Clone)]
pub enum AccessSize {
    Size8,
    Size16,
    Size32,
    Size64,
}

pub trait MemInt: PrimInt+Into<u64> {
    type Half: MemInt;
    const SIZE: usize = ::std::mem::size_of::<Self>();
    const ACCESS_SIZE: AccessSize;
    fn truncate_from(v: u64) -> Self;
    fn endian_read_from<O: ByteOrder>(buf: &[u8]) -> Self;
    fn endian_write_to<O: ByteOrder>(buf: &mut [u8], val: Self);
    fn from_halves<O: ByteOrderCombiner>(before: Self::Half, after: Self::Half) -> Self;
}

impl MemInt for u8 {
    type Half = u8;
    const ACCESS_SIZE: AccessSize = AccessSize::Size8;
    #[inline(always)]
    fn truncate_from(v: u64) -> u8 {
        return v as u8;
    }
    #[inline(always)]
    fn endian_read_from<O: ByteOrder>(buf: &[u8]) -> Self {
        buf[0]
    }
    #[inline(always)]
    fn endian_write_to<O: ByteOrder>(buf: &mut [u8], val: Self) {
        buf[0] = val;
    }
    #[inline(always)]
    fn from_halves<O: ByteOrderCombiner>(_before: Self::Half, _after: Self::Half) -> Self {
        panic!("internal error: u8::from_halves should never be called")
    }
}

impl MemInt for u16 {
    type Half = u8;
    const ACCESS_SIZE: AccessSize = AccessSize::Size16;
    #[inline(always)]
    fn truncate_from(v: u64) -> u16 {
        return v as u16;
    }
    #[inline(always)]
    fn endian_read_from<O: ByteOrder>(buf: &[u8]) -> Self {
        O::read_u16(buf)
    }
    #[inline(always)]
    fn endian_write_to<O: ByteOrder>(buf: &mut [u8], val: Self) {
        O::write_u16(buf, val)
    }
    #[inline(always)]
    fn from_halves<O: ByteOrderCombiner>(before: Self::Half, after: Self::Half) -> Self {
        O::combine16(before, after)
    }
}

impl MemInt for u32 {
    type Half = u16;
    const ACCESS_SIZE: AccessSize = AccessSize::Size32;
    #[inline(always)]
    fn truncate_from(v: u64) -> u32 {
        return v as u32;
    }
    #[inline(always)]
    fn endian_read_from<O: ByteOrder>(buf: &[u8]) -> Self {
        O::read_u32(buf)
    }
    #[inline(always)]
    fn endian_write_to<O: ByteOrder>(buf: &mut [u8], val: Self) {
        O::write_u32(buf, val)
    }
    #[inline(always)]
    fn from_halves<O: ByteOrderCombiner>(before: Self::Half, after: Self::Half) -> Self {
        O::combine32(before, after)
    }
}

impl MemInt for u64 {
    type Half = u32;
    const ACCESS_SIZE: AccessSize = AccessSize::Size64;
    #[inline(always)]
    fn truncate_from(v: u64) -> u64 {
        return v;
    }
    #[inline(always)]
    fn endian_read_from<O: ByteOrder>(buf: &[u8]) -> Self {
        O::read_u64(buf)
    }
    #[inline(always)]
    fn endian_write_to<O: ByteOrder>(buf: &mut [u8], val: Self) {
        O::write_u64(buf, val)
    }
    #[inline(always)]
    fn from_halves<O: ByteOrderCombiner>(before: Self::Half, after: Self::Half) -> Self {
        O::combine64(before, after)
    }
}

pub trait ByteOrderCombiner : ByteOrder {
    fn combine64(before: u32, after: u32) -> u64;
    fn combine32(before: u16, after: u16) -> u32;
    fn combine16(before: u8, after: u8) -> u16;
    fn subint_mask<U,S>(off: usize) -> (U, usize)
        where U: MemInt, S:MemInt+Into<u64>;
}

impl ByteOrderCombiner for LittleEndian {
    #[inline(always)]
    fn subint_mask<U,S>(off: usize) -> (U, usize)
    where
        U: MemInt,
        S: MemInt+Into<u64>,
    {
        let off = off & (U::SIZE-1) & !(S::SIZE-1);
        let shift = off*8;
        let full: u64 = (!S::zero()).into();
        let mask = U::truncate_from(full) << shift;
        (mask, shift)
    }

    #[inline(always)]
    fn combine64(before: u32, after: u32) -> u64 {
        (before as u64) | ((after as u64) << 32)
    }
    #[inline(always)]
    fn combine32(before: u16, after: u16) -> u32 {
        (before as u32) | ((after as u32) << 16)
    }
    #[inline(always)]
    fn combine16(before: u8, after: u8) -> u16 {
        (before as u16) | ((after as u16) << 8)
    }
}

impl ByteOrderCombiner for BigEndian {
    #[inline(always)]
    fn subint_mask<U,S>(off: usize) -> (U, usize)
    where
        U: MemInt,
        S: MemInt+Into<u64>,
    {
        let off = !off & (U::SIZE-1) & !(S::SIZE-1);
        let shift = off*8;
        let full: u64 = (!S::zero()).into();
        let mask = U::truncate_from(full) << shift;
        return (mask, shift)
    }

    #[inline(always)]
    fn combine64(before: u32, after: u32) -> u64 {
        ((before as u64) << 32) | (after as u64)
    }
    #[inline(always)]
    fn combine32(before: u16, after: u16) -> u32 {
        ((before as u32) << 16) | (after as u32)
    }
    #[inline(always)]
    fn combine16(before: u8, after: u8) -> u16 {
        ((before as u16) << 8) | (after as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subint_mask_le() {
        let subint_32_8 = |val, pc| {
            let (mask,shift) = LittleEndian::subint_mask::<u32,u8>(pc);
            (val & mask) >> shift
        };
        let subint_64_16 = |val, pc| {
            let (mask,shift) = LittleEndian::subint_mask::<u64,u16>(pc);
            (val & mask) >> shift
        };
        let subint_32_32 = |val, pc| {
            let (mask,shift) = LittleEndian::subint_mask::<u32,u32>(pc);
            (val & mask) >> shift
        };

        assert_eq!(subint_32_8(0x12345678, 0xFF00), 0x78);
        assert_eq!(subint_32_8(0x12345678, 0xFF01), 0x56);
        assert_eq!(subint_32_8(0x12345678, 0xFF02), 0x34);
        assert_eq!(subint_32_8(0x12345678, 0xFF03), 0x12);

        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF00), 0xCDEF);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF01), 0xCDEF);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF02), 0x89AB);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF03), 0x89AB);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF04), 0x4567);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF05), 0x4567);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF06), 0x0123);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF07), 0x0123);

        assert_eq!(subint_32_32(0x12345678, 0xFF00), 0x12345678);
        assert_eq!(subint_32_32(0x12345678, 0xFF01), 0x12345678);
        assert_eq!(subint_32_32(0x12345678, 0xFF02), 0x12345678);
        assert_eq!(subint_32_32(0x12345678, 0xFF03), 0x12345678);
    }

    #[test]
    fn subint_mask_be() {
        let subint_32_8 = |val, pc| {
            let (mask,shift) = BigEndian::subint_mask::<u32,u8>(pc);
            (val & mask) >> shift
        };
        let subint_64_16 = |val, pc| {
            let (mask,shift) = BigEndian::subint_mask::<u64,u16>(pc);
            (val & mask) >> shift
        };
        let subint_32_32 = |val, pc| {
            let (mask,shift) = BigEndian::subint_mask::<u32,u32>(pc);
            (val & mask) >> shift
        };

        assert_eq!(subint_32_8(0x12345678, 0xFF00), 0x12);
        assert_eq!(subint_32_8(0x12345678, 0xFF01), 0x34);
        assert_eq!(subint_32_8(0x12345678, 0xFF02), 0x56);
        assert_eq!(subint_32_8(0x12345678, 0xFF03), 0x78);

        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF07), 0xCDEF);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF06), 0xCDEF);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF05), 0x89AB);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF04), 0x89AB);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF03), 0x4567);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF02), 0x4567);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF01), 0x0123);
        assert_eq!(subint_64_16(0x0123456789ABCDEF, 0xFF00), 0x0123);

        assert_eq!(subint_32_32(0x12345678, 0xFF00), 0x12345678);
        assert_eq!(subint_32_32(0x12345678, 0xFF01), 0x12345678);
        assert_eq!(subint_32_32(0x12345678, 0xFF02), 0x12345678);
        assert_eq!(subint_32_32(0x12345678, 0xFF03), 0x12345678);
    }
}
