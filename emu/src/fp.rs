extern crate typenum;
use self::typenum::{Cmp, Greater, Less, U0, U2, U32, U5, Unsigned};
use std::fmt;
use std::marker::PhantomData;

pub trait FixedPointInt: Copy + Into<i64> {
    type Len: Unsigned;

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
}

pub trait FixedPoint {
    type BITS: FixedPointInt;
    type FRAC: Unsigned
        + Cmp<U0, Output = Greater>
        + Cmp<<Self::BITS as FixedPointInt>::Len, Output = Less>;

    fn from_bits(bits: Self::BITS) -> Self;
    fn to_f32(&self) -> f32;
}

#[derive(Copy, Clone, Default)]
pub struct Q<BITS, FRAC> {
    bits: BITS,
    phantom: PhantomData<FRAC>,
}

impl<BITS, FRAC> FixedPoint for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Cmp<U0, Output = Greater> + Cmp<BITS::Len, Output = Less>,
{
    type BITS = BITS;
    type FRAC = FRAC;

    fn from_bits(bits: BITS) -> Self {
        Self {
            bits: bits,
            phantom: PhantomData,
        }
    }

    fn to_f32(&self) -> f32 {
        (self.bits.to_i64() as f32) / ((1 << FRAC::to_usize()) as f32)
    }
}

impl<BITS, FRAC> fmt::Debug for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Cmp<U0, Output = Greater> + Cmp<BITS::Len, Output = Less>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Q({0:.3})", self.to_f32())
    }
}

impl<BITS, FRAC> fmt::LowerHex for Q<BITS, FRAC>
where
    BITS: FixedPointInt,
    FRAC: Unsigned + Cmp<U0, Output = Greater> + Cmp<BITS::Len, Output = Less>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Q({:0.*x})", BITS::sizeof() * 2, self.bits.to_u64())
    }
}

pub type I27F5 = Q<i32, U5>;
pub type I30F2 = Q<i32, U2>;
