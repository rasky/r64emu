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

pub trait ColorFormat {
    /// unsigned integer word (eg: u16)
    type U: MemInt;
    /// numer of red bits
    type RN: Unsigned;
    /// shift amount for red bits
    type RS: Unsigned;
    /// number of green bits
    type GN: Unsigned;
    /// shift amount for green bits
    type GS: Unsigned;
    /// number of blue bits
    type BN: Unsigned;
    /// shift amount for blue bits
    type BS: Unsigned;
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct cf<U, RN, RS, GN, GS, BN, BS> {
    phantom: PhantomData<(U, RN, RS, GN, GS, BN, BS)>,
}

impl<U, RN, RS, GN, GS, BN, BS> ColorFormat for cf<U, RN, RS, GN, GS, BN, BS>
where
    U: MemInt,
    RN: Unsigned,
    RS: Unsigned,
    GN: Unsigned,
    GS: Unsigned,
    BN: Unsigned,
    BS: Unsigned,
{
    type U = U;
    type RN = RN;
    type RS = RS;
    type GN = GN;
    type GS = GS;
    type BN = BN;
    type BS = BS;
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub struct Color<CF: ColorFormat> {
    r: Value<CF::U, CF::RN, CF::RS>,
    g: Value<CF::U, CF::GN, CF::GS>,
    b: Value<CF::U, CF::BN, CF::BS>,
}

impl<CF: ColorFormat> Color<CF> {
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

    pub fn from_bits(val: CF::U) -> Self {
        Self {
            r: Component::from_bits(val),
            g: Component::from_bits(val),
            b: Component::from_bits(val),
        }
    }

    pub fn to_bits(&self) -> CF::U {
        self.r.to_bits() | self.g.to_bits() | self.b.to_bits()
    }

    pub fn from<CF2: ColorFormat>(c: Color<CF2>) -> Self {
        Self {
            r: c.r.into(),
            g: c.g.into(),
            b: c.b.into(),
        }
    }

    pub fn into<CF2: ColorFormat>(self) -> Color<CF2> {
        Color::from(self)
    }
}

impl<CF: ColorFormat> fmt::Debug for Color<CF> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!(
            "Rgb{}{}{}",
            CF::RN::to_usize(),
            CF::GN::to_usize(),
            CF::BN::to_usize()
        )).field("r", &self.r.val)
            .field("g", &self.g.val)
            .field("b", &self.b.val)
            .finish()
    }
}

pub type Rgb555 = cf<u16, U5, U0, U5, U5, U5, U10>;
pub type Rgb565 = cf<u16, U5, U0, U6, U5, U5, U11>;
pub type Rgb888 = cf<u32, U8, U0, U8, U8, U8, U16>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color() {
        assert_eq!(Color::<Rgb565>::new(0x10, 0x10, 0x10).is_some(), true);
        assert_eq!(Color::<Rgb565>::new(0x1F, 0x3F, 0x1F).is_some(), true);
        assert_eq!(Color::<Rgb565>::new(0x1F, 0x40, 0x1F).is_some(), false);

        let c1 = Color::<Rgb888>::new_clamped(0xAA, 0x77, 0x33);
        let c2: Color<Rgb555> = c1.into();
        assert_eq!(
            Color::<Rgb555>::new(0xAA >> 3, 0x77 >> 3, 0x33 >> 3).unwrap(),
            c2
        );

        let c1 = Color::<Rgb565>::new_clamped(0x13, 0x24, 0x14);
        let c2: Color<Rgb888> = c1.into();
        assert_eq!(
            Color::<Rgb888>::new(
                (0x13 << 3) | (0x13 >> 2),
                (0x24 << 2) | (0x24 >> 4),
                (0x14 << 3) | (0x14 >> 2)
            ).unwrap(),
            c2
        );
    }
}
