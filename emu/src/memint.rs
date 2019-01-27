//! A module to simplify writing generic code parametrized on different memory
//! access sizes (8, 16, 32, 64 bits).
//!
//! Many CPUs can access their bus with different access sizes (from 8 to 64 bits).
//! While writing emulators, it is common to write code that is similar between
//! access sizes. This module defines a [`MemInt`](trait.MemInt.html) trait
//! that is useful to write generic code in Rust that takes memory access size
//! as generic parameter.

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use enum_map::Enum;
use num::PrimInt;
use serde::{Deserialize, Serialize};

/// An enum that defines the four supported access sizes. This is exposed
/// through the associated constant `ACCESS_SIZE` in
/// [`MemInt`](trait.MemInt.html). This enum is also using the `enum_map` crate,
/// so that it can be used in a very efficient [`EnumMap`](struct.EnumMap.html)
/// (which boils down to a 4-element array) in case there is a need for a
/// runtime data structure indexed by access size.
#[derive(Debug, Enum, Copy, Clone, PartialEq, Eq)]
pub enum AccessSize {
    Size8,
    Size16,
    Size32,
    Size64,
}

/// MemInt is a trait that exposes useful methods for writing generic code
/// that is parametrized on access size. See module-level documentation for more
/// information.
///
/// MemInt is implemented for: `u8`, `u16`, `u32`, `u64`.
pub trait MemInt: PrimInt + Into<u64> + Default + Serialize + Deserialize<'static> {
    /// `Half` is an associated type that holds the integer type whose size is
    /// half of the current one. For instance, `u32::Half` is `u16`. Notice that
    /// `u8::Half` is still `u8`, which is an approximation because there's no
    /// way to represent a 4-bit integer.
    type Half: MemInt + Into<Self>;

    /// `SIZE` is an associated constant that holds the size in bytes of the integer.
    /// For instance, `u16::SIZE` is 2.
    const SIZE: usize = ::std::mem::size_of::<Self>();

    /// `SIZE_LOG` is the log2 of SIZE.
    const SIZE_LOG: usize;

    /// `ACCESS_SIZE` is an associated constant of [`enum
    /// AccessSize`](enum.AccessSize.html) type, that can be used together with
    /// an `EnumMap` to create a runtime data structure indexed by memory access
    /// size.
    const ACCESS_SIZE: AccessSize;

    /// Convert a `u64` into the current type, truncating the value. Notice that
    /// you can use the `Into` trait to do the opposite operation, as `MemInt`
    /// requires `Into<u64>`.
    fn truncate_from(v: u64) -> Self;

    /// Read an integer from a memory buffer with the specified `ByteOrder`
    /// (endianess).
    fn endian_read_from<O: ByteOrder>(buf: &[u8]) -> Self;

    /// Write an integer to a memory buffer with the specified `ByteOrder`
    /// (endianess).
    fn endian_write_to<O: ByteOrder>(buf: &mut [u8], val: Self);

    /// Create an integer composing two halves (low part and high part). The
    /// specified `ByteOrderCombiner` (endianess) commands how the combination
    /// works: if `LittleEndian`, `before` is the low part, and `after` is the
    /// high part; if `BigEndian`, `before` is the hi part, and `after` is the
    /// low part.
    fn from_halves<O: ByteOrderCombiner>(before: Self::Half, after: Self::Half) -> Self;
}

impl MemInt for u8 {
    type Half = u8;
    const ACCESS_SIZE: AccessSize = AccessSize::Size8;
    const SIZE_LOG: usize = 0;
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
    const SIZE_LOG: usize = 1;
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
    const SIZE_LOG: usize = 2;
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
    const SIZE_LOG: usize = 3;
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

/// `ByteOrderCombiner` is a trait that extends `ByteOrder`, providing additional
/// functionalities not implemented by the `byteorder` crate. It is implemented
/// for both `LittleEndian` and `BigEndian`.
pub trait ByteOrderCombiner: ByteOrder {
    /// Convert a [`MemInt`](trait.MemInt.html) integer to native byte order.
    fn to_native<U: MemInt>(val: U) -> U;

    /// Combine two 32-bit halves into a 64-bit integer.
    fn combine64(before: u32, after: u32) -> u64;

    /// Combine two 16-bit halves into a 32-bit integer.
    fn combine32(before: u16, after: u16) -> u32;

    /// Combine two 8-bit halves into a 16-bit integer.
    fn combine16(before: u8, after: u8) -> u16;

    fn subint_mask<U, S>(off: usize) -> (U, usize)
    where
        U: MemInt,
        S: MemInt + Into<U>;
}

impl ByteOrderCombiner for LittleEndian {
    #[inline(always)]
    fn subint_mask<U, S>(off: usize) -> (U, usize)
    where
        U: MemInt,
        S: MemInt + Into<U>,
    {
        let off = off & (U::SIZE - 1) & !(S::SIZE - 1);
        let shift = off * 8;
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
    #[inline(always)]
    fn to_native<U: MemInt>(val: U) -> U {
        U::from_le(val)
    }
}

impl ByteOrderCombiner for BigEndian {
    #[inline(always)]
    fn subint_mask<U, S>(off: usize) -> (U, usize)
    where
        U: MemInt,
        S: MemInt + Into<U>,
    {
        let off = !off & (U::SIZE - 1) & !(S::SIZE - 1);
        let shift = off * 8;
        let full: u64 = (!S::zero()).into();
        let mask = U::truncate_from(full) << shift;
        return (mask, shift);
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
    #[inline(always)]
    fn to_native<U: MemInt>(val: U) -> U {
        U::from_be(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subint_mask_le() {
        let subint_32_8 = |val, pc| {
            let (mask, shift) = LittleEndian::subint_mask::<u32, u8>(pc);
            (val & mask) >> shift
        };
        let subint_64_16 = |val, pc| {
            let (mask, shift) = LittleEndian::subint_mask::<u64, u16>(pc);
            (val & mask) >> shift
        };
        let subint_32_32 = |val, pc| {
            let (mask, shift) = LittleEndian::subint_mask::<u32, u32>(pc);
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
            let (mask, shift) = BigEndian::subint_mask::<u32, u8>(pc);
            (val & mask) >> shift
        };
        let subint_64_16 = |val, pc| {
            let (mask, shift) = BigEndian::subint_mask::<u64, u16>(pc);
            (val & mask) >> shift
        };
        let subint_32_32 = |val, pc| {
            let (mask, shift) = BigEndian::subint_mask::<u32, u32>(pc);
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
