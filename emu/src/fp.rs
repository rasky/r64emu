extern crate num;
extern crate typenum;
use self::num::PrimInt;
use self::typenum::{Cmp, Greater, Less, U0, U10, U2, U29, U32, U5, Unsigned};
use std::fmt;
use std::marker::PhantomData;
use std::ops;

pub trait FixedPointInt: PrimInt + Into<i64> {
    type Len: Unsigned;

    fn truncate_from(val: i64) -> Self;
    fn truncate_from_f32(val: f32) -> Self;
    fn sizeof() -> usize {
        Self::Len::to_usize()
    }
    fn to_i64(self) -> i64 {
        self.into()
    }
    fn to_u64(self) -> u64 {
        self.to_i64() as u64
    }
}

impl FixedPointInt for i32 {
    type Len = U32;
    fn truncate_from(val: i64) -> i32 {
        val as i32
    }
    fn truncate_from_f32(val: f32) -> i32 {
        val as i32
    }
}

pub trait FixedPoint: Copy {
    type BITS: FixedPointInt;
    type FRAC: Unsigned
        + Copy
        + Cmp<U0, Output = Greater>
        + Cmp<<Self::BITS as FixedPointInt>::Len, Output = Less>;

    #[inline(always)]
    fn from_int(int: Self::BITS) -> Self;

    #[inline(always)]
    fn from_f32(v: f32) -> Self;

    #[inline(always)]
    fn from_bits(bits: Self::BITS) -> Self;

    #[inline(always)]
    fn to_f32(self) -> f32;

    #[inline(always)]
    fn floor(self) -> Self::BITS;

    #[inline(always)]
    fn round(self) -> Self::BITS;

    #[inline(always)]
    fn into<BITS2, FRAC2>(self) -> Q<BITS2, FRAC2>
    where
        BITS2: FixedPointInt,
        FRAC2: Unsigned + Copy + Cmp<U0, Output = Greater> + Cmp<BITS2::Len, Output = Less>;
}

#[derive(Copy, Clone, Default)]
pub struct Q<BITS, FRAC> {
    bits: BITS,
    phantom: PhantomData<FRAC>,
}

impl<BITS, FRAC> FixedPoint for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + Cmp<U0, Output = Greater> + Cmp<BITS::Len, Output = Less>,
{
    type BITS = BITS;
    type FRAC = FRAC;

    #[inline(always)]
    fn from_int(v: BITS) -> Self {
        let bits = v << FRAC::to_usize();
        if bits >> FRAC::to_usize() != v {
            panic!("fixed point overflow")
        }
        Self {
            bits: bits,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn from_f32(v: f32) -> Self {
        let bits = BITS::truncate_from_f32(v * (1 << FRAC::to_usize()) as f32);
        let int: i64 = bits.into();
        if (int >> FRAC::to_usize()) as f32 != v.floor() {
            panic!("fixed point overflow")
        }
        Self {
            bits: bits,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn from_bits(bits: BITS) -> Self {
        Self {
            bits: bits,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn to_f32(self) -> f32 {
        (self.bits.to_i64() as f32) / ((1 << FRAC::to_usize()) as f32)
    }

    #[inline(always)]
    fn floor(self) -> BITS {
        self.bits >> FRAC::to_usize()
    }

    #[inline(always)]
    fn round(self) -> BITS {
        let round = BITS::truncate_from(1i64 << (FRAC::to_usize() - 1));
        (self.bits + round) >> FRAC::to_usize()
    }

    #[inline(always)]
    fn into<BITS2, FRAC2>(self) -> Q<BITS2, FRAC2>
    where
        BITS2: FixedPointInt,
        FRAC2: Unsigned + Copy + Cmp<U0, Output = Greater> + Cmp<BITS2::Len, Output = Less>,
    {
        let bits: BITS2 = BITS2::truncate_from(self.bits.into());

        let bits2 = if FRAC::to_usize() < FRAC2::to_usize() {
            let bits2 = bits << (FRAC2::to_usize() - FRAC::to_usize());
            if (bits2 >> FRAC2::to_usize()) != (bits >> FRAC::to_usize()) {
                panic!("fixed point overflow")
            }
            bits2
        } else {
            bits >> (FRAC::to_usize() - FRAC2::to_usize())
        };

        Q {
            bits: bits2,
            phantom: PhantomData,
        }
    }
}

impl<BITS, FRAC> fmt::Debug for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + Cmp<U0, Output = Greater> + Cmp<BITS::Len, Output = Less>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Q({0:.3})", self.to_f32())
    }
}

impl<BITS, FRAC> fmt::LowerHex for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + Cmp<U0, Output = Greater> + Cmp<BITS::Len, Output = Less>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Q({:0.*x})", BITS::sizeof() * 2, self.bits.to_u64())
    }
}

impl<BITS, FRAC, RHS> ops::Add<RHS> for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + Cmp<U0, Output = Greater> + Cmp<BITS::Len, Output = Less>,
    RHS: FixedPoint,
{
    type Output = Self;

    #[inline(always)]
    fn add(self, other: RHS) -> Self {
        let other: Self = other.into();
        Self {
            bits: self.bits + other.bits,
            phantom: PhantomData,
        }
    }
}

pub type I27F5 = Q<i32, U5>;
pub type I30F2 = Q<i32, U2>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn add_conv() {
        let v = Q::<i32, U10>::from_int(100);
        let v2 = Q::<i32, U5>::from_int(100);

        assert_eq!((v + v2).floor(), 200);
        assert_eq!((v2 + v).floor(), 200);
    }

    #[test]
    fn add_conv_round() {
        let v = Q::<i32, U10>::from_f32(100.6);
        let v2 = Q::<i32, U5>::from_f32(100.3);

        assert_eq!((v + v2).round(), 201);
        assert_eq!((v2 + v).round(), 201);
    }

    #[test]
    #[should_panic]
    fn from_int_error() {
        let v = Q::<i32, U29>::from_int(100);
    }

    #[test]
    #[should_panic]
    fn from_f32_error() {
        let v = Q::<i32, U29>::from_f32(100.0);
    }

    #[test]
    fn add() {}
}
