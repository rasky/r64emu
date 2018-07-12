extern crate num;
extern crate typenum;
use self::num::cast::NumCast;
use self::num::PrimInt;
use self::typenum::{IsGreater, IsLessOrEqual, True, U0, U10, U2, U29, U32, U5, U64, Unsigned};
use std::fmt;
use std::marker::PhantomData;
use std::ops;

pub trait FixedPointInt: PrimInt {
    type DoubleInt: PrimInt;
    type Len: Unsigned + IsGreater<U0, Output = True>;

    fn sizeof() -> usize {
        Self::Len::to_usize()
    }
}

impl FixedPointInt for i32 {
    type DoubleInt = i64;
    type Len = U32;
}

impl FixedPointInt for i64 {
    type DoubleInt = i128;
    type Len = U64;
}

pub trait FixedPoint: Copy {
    type BITS: FixedPointInt;
    type FRAC: Unsigned + Copy + IsLessOrEqual<<Self::BITS as FixedPointInt>::Len, Output = True>;

    #[inline(always)]
    fn from_int(int: Self::BITS) -> Self;

    #[inline(always)]
    fn from_f32(v: f32) -> Self;

    #[inline(always)]
    fn from_bits(bits: Self::BITS) -> Self;

    #[inline(always)]
    fn to_f32(self) -> f32;

    #[inline(always)]
    fn bits(self) -> Self::BITS;

    #[inline(always)]
    fn floor(self) -> Self::BITS;

    #[inline(always)]
    fn round(self) -> Self::BITS;

    #[inline(always)]
    fn ceil(self) -> Self::BITS;

    #[inline(always)]
    fn truncate(self) -> Q<i64, U0>;

    #[inline(always)]
    fn into<BITS2, FRAC2>(self) -> Q<BITS2, FRAC2>
    where
        BITS2: FixedPointInt,
        FRAC2: Unsigned + Copy + IsLessOrEqual<BITS2::Len, Output = True>;
}

#[derive(Copy, Clone, Default)]
pub struct Q<BITS, FRAC> {
    bits: BITS,
    phantom: PhantomData<FRAC>,
}

impl<BITS, FRAC> FixedPoint for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + IsLessOrEqual<BITS::Len, Output = True>,
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
        let bits = BITS::from(v * (1 << FRAC::to_usize()) as f32).unwrap();
        let int: i64 = bits.to_i64().unwrap();
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
        (self.bits.to_i64().unwrap() as f32) / ((1 << FRAC::to_usize()) as f32)
    }

    #[inline(always)]
    fn bits(self) -> BITS {
        self.bits
    }

    #[inline(always)]
    fn floor(self) -> BITS {
        self.bits >> FRAC::to_usize()
    }

    #[inline(always)]
    fn round(self) -> BITS {
        let round = BITS::from(1i64 << (FRAC::to_usize() - 1)).unwrap();
        (self.bits + round) >> FRAC::to_usize()
    }

    #[inline(always)]
    fn ceil(self) -> BITS {
        let round = BITS::from((1i64 << FRAC::to_usize()) - 1).unwrap();
        (self.bits + round) >> FRAC::to_usize()
    }

    #[inline(always)]
    fn truncate(self) -> Q<i64, U0> {
        FixedPoint::into(self)
    }

    #[inline(always)]
    fn into<BITS2, FRAC2>(self) -> Q<BITS2, FRAC2>
    where
        BITS2: FixedPointInt,
        FRAC2: Unsigned + Copy + IsLessOrEqual<BITS2::Len, Output = True>,
    {
        let bits = BITS2::from(self.bits).unwrap();

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
    FRAC: Unsigned + Copy + IsLessOrEqual<BITS::Len, Output = True>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Q({0:.3})", self.to_f32())
    }
}

impl<BITS, FRAC> fmt::LowerHex for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + IsLessOrEqual<BITS::Len, Output = True>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "Q({:0.*x})",
            BITS::sizeof() * 2,
            self.bits.to_u64().unwrap()
        )
    }
}

impl<BITS, FRAC, RHS> ops::Add<RHS> for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + IsLessOrEqual<BITS::Len, Output = True>,
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

impl<BITS, FRAC, RHS> ops::Sub<RHS> for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + IsLessOrEqual<BITS::Len, Output = True>,
    RHS: FixedPoint,
{
    type Output = Self;

    #[inline(always)]
    fn sub(self, other: RHS) -> Self {
        let other: Self = other.into();
        Self {
            bits: self.bits - other.bits,
            phantom: PhantomData,
        }
    }
}

impl<BITS, FRAC, BITS2, FRAC2> ops::Mul<Q<BITS2, FRAC2>> for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + IsLessOrEqual<BITS::Len, Output = True>,
    BITS2: FixedPointInt,
    FRAC2: Unsigned + Copy + IsLessOrEqual<BITS2::Len, Output = True>,
{
    type Output = Self;

    #[inline(always)]
    fn mul(self, other: Q<BITS2, FRAC2>) -> Self {
        let b1 = <BITS::DoubleInt as NumCast>::from(self.bits).unwrap();
        let b2 = <BITS::DoubleInt as NumCast>::from(other.bits).unwrap();
        Self {
            bits: BITS::from((b1 * b2) >> FRAC2::to_usize()).unwrap(),
            phantom: PhantomData,
        }
    }
}

impl<BITS, FRAC, BITS2, FRAC2> ops::Div<Q<BITS2, FRAC2>> for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy + IsLessOrEqual<BITS::Len, Output = True>,
    BITS2: FixedPointInt,
    FRAC2: Unsigned + Copy + IsLessOrEqual<BITS2::Len, Output = True>,
{
    type Output = Self;

    #[inline(always)]
    fn div(self, other: Q<BITS2, FRAC2>) -> Self {
        let b1 = <BITS::DoubleInt as NumCast>::from(self.bits).unwrap();
        let b2 = <BITS::DoubleInt as NumCast>::from(other.bits).unwrap();

        Self {
            bits: BITS::from((b1 << FRAC2::to_usize()) / b2).unwrap(),
            phantom: PhantomData,
        }
    }
}

pub type I32F0 = Q<i32, U0>;
pub type I30F2 = Q<i32, U2>;
pub type I27F5 = Q<i32, U5>;
pub type I22F10 = Q<i32, U10>;
pub type I3F29 = Q<i32, U29>;

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
        let v = Q::<i32, U10>::from_f32(98.6);
        let v2 = Q::<i32, U5>::from_f32(102.3);

        assert_eq!((v + v2).round(), 201);
        assert_eq!((v2 + v).round(), 201);

        assert_eq!((v - v2).round(), -4);
        assert_eq!((v2 - v).round(), 4);
        assert_eq!((v - v2).floor(), -4); // FIXME?
        assert_eq!((v2 - v).floor(), 3);
        assert_eq!((v - v2).ceil(), -3); // FIXME?
        assert_eq!((v2 - v).ceil(), 4);
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
    fn mul_div() {
        let v = Q::<i32, U10>::from_f32(13.5);
        let v2 = Q::<i32, U5>::from_f32(142.5);
        assert_eq!((v * v2).round(), 1924);
        assert_eq!((v2 / v).round(), 11);

        let v = Q::<i32, U10>::from_f32(13.5);
        let v2 = Q::<i32, U5>::from_f32(-142.5);
        assert_eq!((v * v2).round(), -1924);
        assert_eq!((v2 / v).round(), -11);
    }

    #[test]
    fn truncated() {
        let v = Q::<i32, U0>::from_f32(14.74);
        let v2 = Q::<i32, U10>::from_f32(14.74).truncate();
        assert_eq!(v.bits(), 14);
        assert_eq!(v2.bits(), 14);
    }
}
