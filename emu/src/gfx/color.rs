extern crate byteorder;
extern crate typenum;

#[allow(unused_imports)]
use self::typenum::{
    U0, U1, U10, U11, U12, U13, U14, U15, U16, U17, U18, U19, U2, U20, U21, U22, U23, U24, U25,
    U26, U27, U28, U29, U3, U30, U31, U4, U5, U6, U7, U8, U9, Unsigned,
};
use super::super::bus::MemInt;
use std::fmt;
use std::marker::PhantomData;

trait Component {
    type U: MemInt;
    type NBITS: Unsigned;
    type SHIFT: Unsigned;

    fn from_bits(val: Self::U) -> Self;
    fn to_bits(&self) -> Self::U;
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
struct Value<U: MemInt, N: Unsigned, S: Unsigned> {
    val: i32,
    phantom: PhantomData<(U, N, S)>,
}

impl<U: MemInt, N: Unsigned, S: Unsigned> Value<U, N, S> {
    fn max() -> i32 {
        (1i32 << N::to_usize()) - 1
    }

    fn new<W: Into<i32>>(val: W) -> Option<Self> {
        let val = val.into() as i32;
        if val < 0 || val > Self::max() {
            None
        } else {
            Some(Self {
                val,
                phantom: PhantomData,
            })
        }
    }

    fn new_clamped<W: Into<i32>>(val: W) -> Self {
        let mut val = val.into() as i32;
        if val < 0 {
            val = 0;
        } else if val > Self::max() {
            val = Self::max();
        }
        Self {
            val,
            phantom: PhantomData,
        }
    }

    fn from<U1: MemInt, N1: Unsigned, S1: Unsigned>(v: Value<U1, N1, S1>) -> Self {
        if N::to_usize() < N1::to_usize() {
            Self {
                val: (v.val >> (N1::to_usize() - N::to_usize())),
                phantom: PhantomData,
            }
        } else {
            Self {
                // TODO: this formula doesn't work for all bit sizes, but only for common one.
                // We're basically doing (v<<3)|(v>>2) when converting 5 bits to 8 bits.
                val: (v.val << (N::to_usize() - N1::to_usize()))
                    | (v.val >> (N1::to_usize() - (N::to_usize() - N1::to_usize()))),
                phantom: PhantomData,
            }
        }
    }

    fn into<U1: MemInt, N1: Unsigned, S1: Unsigned>(self) -> Value<U1, N1, S1> {
        Value::from(self)
    }
}

impl<U: MemInt, N: Unsigned, S: Unsigned> Component for Value<U, N, S> {
    type U = U;
    type NBITS = N;
    type SHIFT = S;

    fn from_bits(val: U) -> Self {
        let val: i32 = (val >> Self::SHIFT::to_usize()).into() as i32;
        Self {
            val: val & Self::max(),
            phantom: PhantomData,
        }
    }

    fn to_bits(&self) -> U {
        U::truncate_from(self.val as u64) << Self::SHIFT::to_usize()
    }
}

pub trait Color {
    type U: MemInt;

    fn from_bits(val: Self::U) -> Self;
    fn to_bits(&self) -> Self::U;
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub struct Rgb<
    U: MemInt,
    RN: Unsigned,
    RS: Unsigned,
    GN: Unsigned,
    GS: Unsigned,
    BN: Unsigned,
    BS: Unsigned,
> {
    r: Value<U, RN, RS>,
    g: Value<U, GN, GS>,
    b: Value<U, BN, BS>,
}

impl<
        U: MemInt,
        RN: Unsigned,
        RS: Unsigned,
        GN: Unsigned,
        GS: Unsigned,
        BN: Unsigned,
        BS: Unsigned,
    > Rgb<U, RN, RS, GN, GS, BN, BS>
{
    pub fn new<W: Into<i32>>(r: W, g: W, b: W) -> Option<Self> {
        Some(Self {
            r: match Value::new(r) {
                Some(r) => r,
                None => return None,
            },
            g: match Value::new(g) {
                Some(r) => r,
                None => return None,
            },
            b: match Value::new(b) {
                Some(b) => b,
                None => return None,
            },
        })
    }

    pub fn new_clamped<W: Into<i32>>(r: W, g: W, b: W) -> Self {
        Self {
            r: Value::new_clamped(r),
            g: Value::new_clamped(g),
            b: Value::new_clamped(b),
        }
    }

    pub fn from<
        U1: MemInt,
        RN1: Unsigned,
        RS1: Unsigned,
        GN1: Unsigned,
        GS1: Unsigned,
        BN1: Unsigned,
        BS1: Unsigned,
    >(
        c: Rgb<U1, RN1, RS1, GN1, GS1, BN1, BS1>,
    ) -> Self {
        Self {
            r: c.r.into(),
            g: c.g.into(),
            b: c.b.into(),
        }
    }

    pub fn into<
        U1: MemInt,
        RN1: Unsigned,
        RS1: Unsigned,
        GN1: Unsigned,
        GS1: Unsigned,
        BN1: Unsigned,
        BS1: Unsigned,
    >(
        self,
    ) -> Rgb<U1, RN1, RS1, GN1, GS1, BN1, BS1> {
        Rgb::from(self)
    }
}

impl<
        U: MemInt,
        RN: Unsigned,
        RS: Unsigned,
        GN: Unsigned,
        GS: Unsigned,
        BN: Unsigned,
        BS: Unsigned,
    > Color for Rgb<U, RN, RS, GN, GS, BN, BS>
{
    type U = U;

    fn from_bits(val: U) -> Self {
        Self {
            r: Component::from_bits(val),
            g: Component::from_bits(val),
            b: Component::from_bits(val),
        }
    }

    fn to_bits(&self) -> U {
        self.r.to_bits() | self.g.to_bits() | self.b.to_bits()
    }
}

impl<
        U: MemInt,
        RN: Unsigned,
        RS: Unsigned,
        GN: Unsigned,
        GS: Unsigned,
        BN: Unsigned,
        BS: Unsigned,
    > fmt::Debug for Rgb<U, RN, RS, GN, GS, BN, BS>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!(
            "Rgb{}{}{}",
            RN::to_usize(),
            GN::to_usize(),
            BN::to_usize()
        )).field("r", &self.r.val)
            .field("g", &self.g.val)
            .field("b", &self.b.val)
            .finish()
    }
}

pub type Rgb555 = Rgb<u16, U5, U0, U5, U5, U5, U10>;
pub type Rgb565 = Rgb<u16, U5, U0, U6, U5, U5, U11>;
pub type Rgb888 = Rgb<u32, U8, U0, U8, U8, U8, U16>;
