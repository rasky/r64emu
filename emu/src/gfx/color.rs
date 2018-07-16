extern crate byteorder;
extern crate typenum;

#[allow(unused_imports)]
use self::typenum::{
    IsEqual, IsGreaterOrEqual, True, U0, U1, U10, U11, U12, U13, U14, U15, U16, U17, U18, U19, U2,
    U20, U21, U22, U23, U24, U25, U26, U27, U28, U29, U3, U30, U31, U32, U4, U5, U6, U7, U8, U9,
    Unsigned,
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

    fn from_f32_clamped(mut val: f32) -> Self {
        if val > 1.0 {
            val = 1.0;
        } else if val < 0.0 {
            val = 0.0;
        }
        let max = Self::max() as f32;
        Self {
            val: (val * max) as i32,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn from<U1: MemInt, N1: Unsigned, S1: Unsigned>(v: Value<U1, N1, S1>) -> Self {
        let max = (1 << N::to_usize()) - 1;
        Self {
            phantom: PhantomData,
            val: match (N1::to_usize(), N::to_usize()) {
                // Special case for alpha 1-bit conversion.
                (1, _) => if v.val == 0 {
                    0
                } else {
                    max
                },

                // Special case for alpha 1-bit conversion.
                (_, 1) => if v.val != 0 {
                    1
                } else {
                    0
                },

                // Special case for alpha elision.
                (0, _) => max,
                (_, 0) => 0,

                // Normal cases
                (f, t) if f == t => v.val,
                (f, t) if f >= t => (v.val >> (f - t)),
                (f, t) if f < t => {
                    // TODO: this formula doesn't work for all bit sizes, but only for common one.
                    // We're basically doing (v<<3)|(v>>2) when converting 5 bits to 8 bits.
                    (v.val << (t - f)) | (v.val >> (f - (t - f)))
                }
                (_, _) => unimplemented!(),
            },
        }
    }

    fn into<U1: MemInt, N1: Unsigned, S1: Unsigned>(self) -> Value<U1, N1, S1> {
        Value::from(self)
    }

    fn as_f32(self) -> f32 {
        (self.val as f32) / (Self::max() as f32)
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

pub trait ColorFormat: Copy {
    /// unsigned integer word (eg: u16)
    type U: MemInt;
    /// actually used bits in word
    type BITS: Unsigned + Copy;
    /// numer of red bits
    type RN: Unsigned + Copy;
    /// shift amount for red bits
    type RS: Unsigned + Copy;
    /// number of green bits
    type GN: Unsigned + Copy;
    /// shift amount for green bits
    type GS: Unsigned + Copy;
    /// number of blue bits
    type BN: Unsigned + Copy;
    /// shift amount for blue bits
    type BS: Unsigned + Copy;
    /// number of alpha bits
    type AN: Unsigned + Copy;
    /// shift amount for alpha bits
    type AS: Unsigned + Copy;
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct cf<U, BITS, RN, RS, GN, GS, BN, BS, AN, AS> {
    phantom: PhantomData<(U, BITS, RN, RS, GN, GS, BN, BS, AN, AS)>,
}

impl<U, BITS, RN, RS, GN, GS, BN, BS, AN, AS> ColorFormat
    for cf<U, BITS, RN, RS, GN, GS, BN, BS, AN, AS>
where
    U: MemInt,
    BITS: Unsigned + Copy,
    RN: Unsigned + Copy,
    RS: Unsigned + Copy,
    GN: Unsigned + Copy,
    GS: Unsigned + Copy,
    BN: Unsigned + Copy,
    BS: Unsigned + Copy,
    AN: Unsigned + Copy,
    AS: Unsigned + Copy,
{
    type U = U;
    type BITS = BITS;
    type RN = RN;
    type RS = RS;
    type GN = GN;
    type GS = GS;
    type BN = BN;
    type BS = BS;
    type AN = AN;
    type AS = AS;
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub struct Color<CF: ColorFormat> {
    r: Value<CF::U, CF::RN, CF::RS>,
    g: Value<CF::U, CF::GN, CF::GS>,
    b: Value<CF::U, CF::BN, CF::BS>,
    a: Value<CF::U, CF::AN, CF::AS>,
}

impl<CF: ColorFormat> Color<CF> {
    pub fn new<W: Into<i32>>(r: W, g: W, b: W, a: W) -> Option<Self> {
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
            a: match Value::new(a) {
                Some(a) => a,
                None => return None,
            },
        })
    }

    pub fn new_clamped<W: Into<i32>>(r: W, g: W, b: W, a: W) -> Self {
        Self {
            r: Value::new_clamped(r),
            g: Value::new_clamped(g),
            b: Value::new_clamped(b),
            a: Value::new_clamped(a),
        }
    }

    pub fn from_f32_clamped(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: Value::from_f32_clamped(r),
            g: Value::from_f32_clamped(g),
            b: Value::from_f32_clamped(b),
            a: Value::from_f32_clamped(a),
        }
    }

    pub fn from_bits(val: CF::U) -> Self {
        Self {
            r: Component::from_bits(val),
            g: Component::from_bits(val),
            b: Component::from_bits(val),
            a: Component::from_bits(val),
        }
    }

    pub fn to_bits(&self) -> CF::U {
        self.r.to_bits() | self.g.to_bits() | self.b.to_bits() | self.a.to_bits()
    }
}

impl<CF: ColorFormat> fmt::Debug for Color<CF> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!(
            "Rgb{}{}{}{}",
            CF::RN::to_usize(),
            CF::GN::to_usize(),
            CF::BN::to_usize(),
            CF::AN::to_usize()
        )).field("r", &self.r.val)
            .field("g", &self.g.val)
            .field("b", &self.b.val)
            .field("a", &self.a.val)
            .finish()
    }
}

pub type I4 = cf<u8, U4, U4, U0, U0, U0, U0, U0, U0, U0>;
pub type I8 = cf<u8, U8, U8, U0, U0, U0, U0, U0, U0, U0>;
pub type Rgb555 = cf<u16, U16, U5, U0, U5, U5, U5, U10, U0, U0>;
pub type Rgb565 = cf<u16, U16, U5, U0, U6, U5, U5, U11, U0, U0>;
pub type Rgb888 = cf<u32, U32, U8, U0, U8, U8, U8, U16, U0, U0>;
pub type Rgba5551 = cf<u16, U16, U5, U0, U5, U5, U5, U10, U1, U15>;
pub type Rgba8888 = cf<u32, U32, U8, U0, U8, U8, U8, U16, U8, U24>;
pub type Abgr8888 = cf<u32, U32, U8, U24, U8, U16, U8, U8, U8, U0>;

pub trait ColorConverter<CF2: ColorFormat>: Sized {
    #[inline(always)]
    fn cconv(self) -> Color<CF2>;
}

impl<CF1: ColorFormat, CF2: ColorFormat> ColorConverter<CF2> for Color<CF1> {
    #[inline(always)]
    default fn cconv(self) -> Color<CF2> {
        Color {
            r: self.r.into(),
            g: self.g.into(),
            b: self.b.into(),
            a: self.a.into(),
        }
    }
}

macro_rules! define_greyscale_conversions {
    ($cfgrey:ident, $($cf2:ident),+) => {
        $(
            impl ColorConverter<$cf2> for Color<$cfgrey> {
                #[inline(always)]
                fn cconv(self) -> Color<$cf2> {
                    Color {
                        r: self.r.into(),
                        g: self.r.into(),
                        b: self.r.into(),
                        a: Value::new_clamped(255),
                    }
                }
            }
            impl ColorConverter<$cfgrey> for Color<$cf2> {
                #[inline(always)]
                fn cconv(self) -> Color<$cfgrey> {
                    let i = self.r.as_f32() * 0.30 + self.g.as_f32() * 0.59 + self.b.as_f32() * 0.11;
                    Color::from_f32_clamped(i, 0.0, 0.0, 0.0)
                }
            }
        )+
    };
}

define_greyscale_conversions!(I4, Rgba8888, Rgba5551, Rgb888, Rgb565, Rgb555);
define_greyscale_conversions!(I8, Rgba8888, Rgba5551, Rgb888, Rgb565, Rgb555);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color() {
        assert_eq!(Color::<Rgb565>::new(0x10, 0x10, 0x10, 0).is_some(), true);
        assert_eq!(Color::<Rgb565>::new(0x1F, 0x3F, 0x1F, 0).is_some(), true);
        assert_eq!(Color::<Rgb565>::new(0x1F, 0x40, 0x1F, 0).is_some(), false);

        let c1 = Color::<Rgb888>::new_clamped(0xAA, 0x77, 0x33, 0);
        let c2: Color<Rgb555> = c1.cconv();
        assert_eq!(
            Color::<Rgb555>::new(0xAA >> 3, 0x77 >> 3, 0x33 >> 3, 0).unwrap(),
            c2
        );

        let c1 = Color::<Rgb565>::new_clamped(0x13, 0x24, 0x14, 0);
        let c2: Color<Rgb888> = c1.cconv();
        assert_eq!(
            Color::<Rgb888>::new(
                (0x13 << 3) | (0x13 >> 2),
                (0x24 << 2) | (0x24 >> 4),
                (0x14 << 3) | (0x14 >> 2),
                0
            ).unwrap(),
            c2
        );
    }

    #[test]
    fn alpha() {
        assert_eq!(Color::<Rgb555>::new(0x10, 0x10, 0x10, 0).is_some(), true);
        assert_eq!(Color::<Rgb555>::new(0x10, 0x10, 0x10, 1).is_none(), true);
        assert_eq!(Color::<Rgba5551>::new(0x10, 0x10, 0x10, 1).is_some(), true);
        assert_eq!(Color::<Rgba5551>::new(0x10, 0x10, 0x10, 2).is_none(), true);

        assert_eq!(Color::<Rgb888>::new(0x10, 0x10, 0x10, 0x10).is_none(), true);
        assert_eq!(
            Color::<Rgba8888>::new(0x10, 0x10, 0x10, 0x10).is_some(),
            true
        );

        // Check conversions semantics for 1-bit alpha
        assert_eq!(
            Color::<Rgba5551>::new_clamped(0, 0, 0, 1).cconv(),
            Color::<Rgba8888>::new_clamped(0, 0, 0, 255)
        );
        assert_eq!(
            Color::<Rgba5551>::new_clamped(0, 0, 0, 0).cconv(),
            Color::<Rgba8888>::new_clamped(0, 0, 0, 0)
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0, 0, 0, 255).cconv(),
            Color::<Rgba5551>::new_clamped(0, 0, 0, 1),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0, 0, 0, 254).cconv(),
            Color::<Rgba5551>::new_clamped(0, 0, 0, 1),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0, 0, 0, 128).cconv(),
            Color::<Rgba5551>::new_clamped(0, 0, 0, 1),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0, 0, 0, 127).cconv(),
            Color::<Rgba5551>::new_clamped(0, 0, 0, 1),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0, 0, 0, 1).cconv(),
            Color::<Rgba5551>::new_clamped(0, 0, 0, 1),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0, 0, 0, 0).cconv(),
            Color::<Rgba5551>::new_clamped(0, 0, 0, 0),
        );

        // Check conversions semantics for multi-bit alpha
        assert_eq!(
            Color::<Rgb888>::new_clamped(0x20, 0x30, 0x40, 0).cconv(),
            Color::<Rgba8888>::new_clamped(0x20, 0x30, 0x40, 255)
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0x20, 0x30, 0x40, 255).cconv(),
            Color::<Rgb888>::new_clamped(0x20, 0x30, 0x40, 0),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0x20, 0x30, 0x40, 128).cconv(),
            Color::<Rgb888>::new_clamped(0x20, 0x30, 0x40, 0),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0x20, 0x30, 0x40, 127).cconv(),
            Color::<Rgb888>::new_clamped(0x20, 0x30, 0x40, 0),
        );
        assert_eq!(
            Color::<Rgba8888>::new_clamped(0x20, 0x30, 0x40, 0).cconv(),
            Color::<Rgb888>::new_clamped(0x20, 0x30, 0x40, 0),
        );

        // Final conversion test
        let c1 = Color::<Rgba5551>::new_clamped(0x13, 0x8, 0x14, 1);
        assert_eq!(c1.to_bits(), 0x13 | (0x8 << 5) | (0x14 << 10) | (1 << 15));

        let c2 = c1.cconv();
        assert_eq!(
            c2.to_bits(),
            ((0x13 << 3) | (0x13 >> 2))
                | (((0x8 << 3) | (0x8 >> 2)) << 8)
                | (((0x14 << 3) | (0x14 >> 2)) << 16)
                | (255 << 24)
        );
        assert_eq!(
            c2,
            Color::<Rgba8888>::new(
                (0x13 << 3) | (0x13 >> 2),
                (0x8 << 3) | (0x8 >> 2),
                (0x14 << 3) | (0x14 >> 2),
                255,
            ).unwrap(),
        );

        let c3 = c2.cconv();
        assert_eq!(c1, c3);
    }

    #[test]
    fn intensity() {
        assert_eq!(Color::<I8>::new(0x80, 0x0, 0x0, 0x0).is_some(), true);
        assert_eq!(Color::<I8>::new(0x80, 0x0, 0x1, 0x0).is_some(), false);
        assert_eq!(Color::<I8>::new(0x80, 0x1, 0x0, 0x0).is_some(), false);
        assert_eq!(Color::<I8>::new(0x80, 0x0, 0x0, 0x1).is_some(), false);

        let v1 = Color::<I8>::new(0x80, 0x0, 0x0, 0x0).unwrap();
        let v2: Color<Rgb888> = v1.cconv();
        assert_eq!(v2, Color::<Rgb888>::new_clamped(0x80, 0x80, 0x80, 0));

        let v1 = Color::<I4>::new(0x08, 0x0, 0x0, 0x0).unwrap();
        let v2: Color<Rgb888> = v1.cconv();
        assert_eq!(v2, Color::<Rgb888>::new_clamped(0x88, 0x88, 0x88, 0));
    }
}
