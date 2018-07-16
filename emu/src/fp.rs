extern crate num;
extern crate typenum;
use self::num::cast::NumCast;
use self::num::{PrimInt, ToPrimitive, Zero};
use self::typenum::{U0, U128, U16, U32, U64, U8, Unsigned};
use std::fmt;
use std::iter;
use std::marker::PhantomData;
use std::ops;

pub trait FixedPointInt: PrimInt + ToPrimitive + iter::Step {
    type DoubleInt: FixedPointInt;
    type Len: Unsigned;

    #[inline(always)]
    fn cast<FPI: FixedPointInt>(self) -> FPI {
        FPI::from(self).unwrap()
    }

    #[inline(always)]
    fn cast_widen(self) -> Self::DoubleInt {
        <Self::DoubleInt as NumCast>::from(self).unwrap()
    }
}

impl FixedPointInt for i8 {
    type DoubleInt = i16;
    type Len = U8;
}

impl FixedPointInt for i16 {
    type DoubleInt = i32;
    type Len = U16;
}

impl FixedPointInt for i32 {
    type DoubleInt = i64;
    type Len = U32;
}

impl FixedPointInt for i64 {
    type DoubleInt = i128;
    type Len = U64;
}

impl FixedPointInt for i128 {
    type DoubleInt = i128;
    type Len = U128;
}

impl FixedPointInt for u32 {
    type DoubleInt = u64;
    type Len = U32;
}

impl FixedPointInt for u64 {
    type DoubleInt = u128;
    type Len = U64;
}

impl FixedPointInt for u128 {
    type DoubleInt = u128;
    type Len = U128;
}

pub trait FixedPoint: Copy + Clone {
    type BITS: FixedPointInt;
    type FRAC: Unsigned + Copy;

    #[inline(always)]
    fn sizeof() -> usize {
        <Self::BITS as FixedPointInt>::Len::to_usize()
    }

    #[inline(always)]
    fn shift() -> usize {
        Self::FRAC::to_usize()
    }
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct q<BITS, FRAC> {
    phantom: PhantomData<(BITS, FRAC)>,
}

impl<BITS, FRAC> FixedPoint for q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Copy,
{
    type BITS = BITS;
    type FRAC = FRAC;
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Q<FP: FixedPoint> {
    bits: FP::BITS,
}

impl<FP: FixedPoint> Q<FP> {
    #[inline(always)]
    pub fn from_int<N: PrimInt>(v: N) -> Self {
        let v = <FP::BITS as NumCast>::from(v).unwrap();
        let bits = v << FP::shift();
        if bits >> FP::shift() != v {
            panic!("fixed point overflow")
        }
        Self { bits: bits }
    }

    #[inline(always)]
    pub fn from_f32(v: f32) -> Self {
        let bits = <FP::BITS as NumCast>::from(v * (1 << FP::shift()) as f32).unwrap();
        let int: i64 = bits.to_i64().unwrap();
        if (int >> FP::shift()) as f32 != v.floor() {
            panic!("fixed point overflow")
        }
        Self { bits: bits }
    }

    #[inline(always)]
    pub fn from_bits(bits: FP::BITS) -> Self {
        Self { bits: bits }
    }

    #[inline(always)]
    pub fn to_f32(self) -> f32 {
        (self.bits.to_i64().unwrap() as f32) / ((1 << FP::shift()) as f32)
    }

    #[inline(always)]
    pub fn bits(self) -> FP::BITS {
        self.bits
    }

    #[inline(always)]
    pub fn floor(self) -> FP::BITS {
        self.bits >> FP::shift()
    }

    #[inline(always)]
    pub fn round(self) -> FP::BITS {
        let round = <FP::BITS as NumCast>::from(1i64 << (FP::shift() - 1)).unwrap();
        (self.bits + round) >> FP::shift()
    }

    #[inline(always)]
    pub fn ceil(self) -> FP::BITS {
        let round = <FP::BITS as NumCast>::from((1i64 << FP::shift()) - 1).unwrap();
        (self.bits + round) >> FP::shift()
    }

    #[inline(always)]
    pub fn is_negative(self) -> bool {
        self.bits < FP::BITS::zero()
    }

    #[inline(always)]
    pub fn truncate(self) -> Q<impl FixedPoint<BITS = FP::BITS>> {
        self.cast::<q<FP::BITS, U0>>()
    }

    #[inline(always)]
    pub fn from<FP2: FixedPoint>(fp: Q<FP2>) -> Self {
        let bits: FP::BITS = if FP2::shift() > FP::shift() {
            (fp.bits >> (FP2::shift() - FP::shift())).cast()
        } else {
            (fp.bits.cast::<FP::BITS>() << (FP::shift() - FP2::shift()))
        };
        let q = Self::from_bits(bits);
        if q.floor().to_i64().unwrap() != fp.floor().to_i64().unwrap() {
            panic!("fixed point overflow");
        }
        q
    }

    #[inline(always)]
    pub fn cast<FP2: FixedPoint>(self) -> Q<FP2> {
        Q::<FP2>::from(self)
    }
}

impl<FP: FixedPoint> fmt::Debug for Q<FP> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Q({0:.3})", self.to_f32())
    }
}

impl<FP: FixedPoint> fmt::LowerHex for Q<FP> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "Q({:0.*x})",
            FP::sizeof() * 2,
            self.bits.to_u64().unwrap()
        )
    }
}

impl<FP: FixedPoint> Default for Q<FP> {
    fn default() -> Self {
        Self {
            bits: FP::BITS::zero(),
        }
    }
}

impl<FP: FixedPoint, RHS: FixedPoint> ops::Add<Q<RHS>> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn add(self, other: Q<RHS>) -> Self {
        let other: Self = other.cast();
        Self {
            bits: self.bits + other.bits,
        }
    }
}

impl<FP: FixedPoint, BITS: FixedPointInt> ops::Add<BITS> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn add(self, other: BITS) -> Self {
        let other = Self::from_int(<FP::BITS as NumCast>::from(other).unwrap());
        Self {
            bits: self.bits + other.bits,
        }
    }
}

impl<FP: FixedPoint, RHS: FixedPoint> ops::Sub<Q<RHS>> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn sub(self, other: Q<RHS>) -> Self {
        let other: Self = other.cast();
        Self {
            bits: self.bits - other.bits,
        }
    }
}

impl<FP: FixedPoint, BITS: FixedPointInt> ops::Sub<BITS> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn sub(self, other: BITS) -> Self {
        let other = Self::from_int(<FP::BITS as NumCast>::from(other).unwrap());
        Self {
            bits: self.bits - other.bits,
        }
    }
}

impl<FP: FixedPoint, RHS: FixedPoint> ops::Mul<Q<RHS>> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn mul(self, other: Q<RHS>) -> Self {
        let b1 = self.bits.cast_widen();
        let b2 = other.bits.cast();
        Self {
            bits: ((b1 * b2) >> RHS::shift()).cast(),
        }
    }
}

impl<FP: FixedPoint, BITS: FixedPointInt> ops::Mul<BITS> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn mul(self, other: BITS) -> Self {
        let other = <FP::BITS as NumCast>::from(other).unwrap();
        Self {
            bits: self.bits * other,
        }
    }
}

impl<FP: FixedPoint, RHS: FixedPoint> ops::Div<Q<RHS>> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn div(self, other: Q<RHS>) -> Self {
        let b1 = self.bits.cast_widen();
        let b2 = other.bits.cast();
        Self {
            bits: ((b1 << RHS::shift()) / b2).cast(),
        }
    }
}

impl<FP: FixedPoint, BITS: FixedPointInt> ops::Div<BITS> for Q<FP> {
    type Output = Self;

    #[inline(always)]
    fn div(self, other: BITS) -> Self {
        let other = <FP::BITS as NumCast>::from(other).unwrap();
        Self {
            bits: self.bits / other,
        }
    }
}

pub mod formats {
    use super::q;
    use super::typenum::{
        U0, U1, U10, U11, U12, U13, U14, U15, U16, U17, U18, U19, U2, U20, U21, U22, U23, U24, U25,
        U26, U27, U28, U29, U3, U30, U31, U32, U4, U5, U6, U7, U8, U9,
    };

    pub type I16F0 = q<i16, U0>;
    pub type I15F1 = q<i16, U1>;
    pub type I14F2 = q<i16, U2>;
    pub type I13F3 = q<i16, U3>;
    pub type I12F4 = q<i16, U4>;
    pub type I11F5 = q<i16, U5>;
    pub type I10F6 = q<i16, U6>;
    pub type I9F7 = q<i16, U7>;
    pub type I8F8 = q<i16, U8>;
    pub type I7F9 = q<i16, U9>;
    pub type I6F10 = q<i16, U10>;
    pub type I5F11 = q<i16, U11>;
    pub type I4F12 = q<i16, U12>;
    pub type I3F13 = q<i16, U13>;
    pub type I2F14 = q<i16, U14>;
    pub type I1F15 = q<i16, U15>;

    pub type I32F0 = q<i32, U0>;
    pub type I31F1 = q<i32, U1>;
    pub type I30F2 = q<i32, U2>;
    pub type I29F3 = q<i32, U3>;
    pub type I28F4 = q<i32, U4>;
    pub type I27F5 = q<i32, U5>;
    pub type I26F6 = q<i32, U6>;
    pub type I25F7 = q<i32, U7>;
    pub type I24F8 = q<i32, U8>;
    pub type I23F9 = q<i32, U9>;
    pub type I22F10 = q<i32, U10>;
    pub type I21F11 = q<i32, U11>;
    pub type I20F12 = q<i32, U12>;
    pub type I19F13 = q<i32, U13>;
    pub type I18F14 = q<i32, U14>;
    pub type I17F15 = q<i32, U15>;
    pub type I16F16 = q<i32, U16>;
    pub type I15F17 = q<i32, U17>;
    pub type I14F18 = q<i32, U18>;
    pub type I13F19 = q<i32, U19>;
    pub type I12F20 = q<i32, U20>;
    pub type I11F21 = q<i32, U21>;
    pub type I10F22 = q<i32, U22>;
    pub type I9F23 = q<i32, U23>;
    pub type I8F24 = q<i32, U24>;
    pub type I7F25 = q<i32, U25>;
    pub type I6F26 = q<i32, U26>;
    pub type I5F27 = q<i32, U27>;
    pub type I4F28 = q<i32, U28>;
    pub type I3F29 = q<i32, U29>;
    pub type I2F30 = q<i32, U30>;
    pub type I1F31 = q<i32, U31>;

    pub type I33F31 = q<i64, U31>;
    pub type I32F32 = q<i64, U32>;

    pub type U16F0 = q<u16, U0>;
    pub type U15F1 = q<u16, U1>;
    pub type U14F2 = q<u16, U2>;
    pub type U13F3 = q<u16, U3>;
    pub type U12F4 = q<u16, U4>;
    pub type U11F5 = q<u16, U5>;
    pub type U10F6 = q<u16, U6>;
    pub type U9F7 = q<u16, U7>;
    pub type U8F8 = q<u16, U8>;
    pub type U7F9 = q<u16, U9>;
    pub type U6F10 = q<u16, U10>;
    pub type U5F11 = q<u16, U11>;
    pub type U4F12 = q<u16, U12>;
    pub type U3F13 = q<u16, U13>;
    pub type U2F14 = q<u16, U14>;
    pub type U1F15 = q<u16, U15>;
    pub type U0F16 = q<u16, U16>;

    pub type U32F0 = q<u32, U0>;
    pub type U31F1 = q<u32, U1>;
    pub type U30F2 = q<u32, U2>;
    pub type U29F3 = q<u32, U3>;
    pub type U28F4 = q<u32, U4>;
    pub type U27F5 = q<u32, U5>;
    pub type U26F6 = q<u32, U6>;
    pub type U25F7 = q<u32, U7>;
    pub type U24F8 = q<u32, U8>;
    pub type U23F9 = q<u32, U9>;
    pub type U22F10 = q<u32, U10>;
    pub type U21F11 = q<u32, U11>;
    pub type U20F12 = q<u32, U12>;
    pub type U19F13 = q<u32, U13>;
    pub type U18F14 = q<u32, U14>;
    pub type U17F15 = q<u32, U15>;
    pub type U16F16 = q<u32, U16>;
    pub type U15F17 = q<u32, U17>;
    pub type U14F18 = q<u32, U18>;
    pub type U13F19 = q<u32, U19>;
    pub type U12F20 = q<u32, U20>;
    pub type U11F21 = q<u32, U21>;
    pub type U10F22 = q<u32, U22>;
    pub type U9F23 = q<u32, U23>;
    pub type U8F24 = q<u32, U24>;
    pub type U7F25 = q<u32, U25>;
    pub type U6F26 = q<u32, U26>;
    pub type U5F27 = q<u32, U27>;
    pub type U4F28 = q<u32, U28>;
    pub type U3F29 = q<u32, U29>;
    pub type U2F30 = q<u32, U30>;
    pub type U1F31 = q<u32, U31>;
    pub type U0F32 = q<u32, U32>;
}

#[cfg(test)]
mod test {
    use super::formats::*;
    use super::*;

    #[test]
    fn add_conv() {
        let v = Q::<I22F10>::from_int(100);
        let v2 = Q::<I27F5>::from_int(100);

        assert_eq!((v + v2).floor(), 200);
        assert_eq!((v2 + v).floor(), 200);
    }

    #[test]
    fn add_conv_round() {
        let v = Q::<I22F10>::from_f32(98.6);
        let v2 = Q::<I27F5>::from_f32(102.3);

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
        let _v = Q::<I3F29>::from_int(100);
    }

    #[test]
    #[should_panic]
    fn from_f32_error() {
        let _v = Q::<I3F29>::from_f32(100.0);
    }

    #[test]
    fn mul_div() {
        let v = Q::<I22F10>::from_f32(13.5);
        let v2 = Q::<I27F5>::from_f32(142.5);
        assert_eq!((v * v2).round(), 1924);
        assert_eq!((v2 / v).round(), 11);

        let v = Q::<I22F10>::from_f32(13.5);
        let v2 = Q::<I27F5>::from_f32(-142.5);
        assert_eq!((v * v2).round(), -1924);
        assert_eq!((v2 / v).round(), -11);
    }

    #[test]
    fn truncated() {
        let v = Q::<I32F0>::from_f32(14.74);
        let v2 = Q::<I22F10>::from_f32(14.74).truncate();
        assert_eq!(v.bits(), 14);
        assert_eq!(v2.bits(), 14);
    }

    #[test]
    fn upcast() {
        let v = Q::<I32F0>::from_int(111);
        let v2 = v.cast::<I33F31>();
        let v3 = v.cast::<I32F32>();
        assert_eq!(v2.floor(), 111);
        assert_eq!(v3.floor(), 111);

        let v4 = v.cast::<I8F8>();
        assert_eq!(v4.floor(), 111);
    }

    #[test]
    fn math_int() {
        let v = Q::<I22F10>::from_f32(15.75);
        let v2 = (((v + 10) * 4) - 100) / 2;
        assert_eq!(v2.floor(), 1);
        assert_eq!(v2.ceil(), 2);
    }
}
