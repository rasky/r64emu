extern crate byteorder;
extern crate typenum;

use self::byteorder::LittleEndian;
#[allow(unused_imports)]
use self::typenum::{
    U0, U1, U10, U11, U12, U13, U14, U15, U16, U17, U18, U19, U2, U20, U21, U22, U23, U24, U25,
    U26, U27, U28, U29, U3, U30, U31, U4, U5, U6, U7, U8, U9, Unsigned,
};
use super::bus::MemInt;
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

pub struct GfxBuffer<'a, C: Color + Sized> {
    mem: &'a [u8],
    width: usize,
    pitch: usize,
    phantom: PhantomData<C>,
}

pub struct GfxBufferMut<'a, C: Color + Sized> {
    mem: &'a mut [u8],
    width: usize,
    pitch: usize,
    phantom: PhantomData<C>,
}

pub struct GfxLine<'a, C: Color + Sized> {
    mem: &'a [u8],
    phantom: PhantomData<C>,
}

pub struct GfxLineMut<'a, C: Color + Sized> {
    mem: &'a mut [u8],
    phantom: PhantomData<C>,
}

pub struct OwnedGfxBuffer<C: Color + Sized> {
    mem: Vec<u8>,
    width: usize,
    height: usize,
    phantom: PhantomData<C>,
}

impl<'a: 's, 's, C: Color + Sized> GfxBuffer<'a, C> {
    pub fn new(
        mem: &'a [u8],
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Option<GfxBuffer<'a, C>> {
        if pitch < width * C::U::SIZE {
            return None;
        }
        if mem.len() < height * pitch {
            return None;
        }
        Some(Self {
            mem: &mem[..height * pitch],
            width,
            pitch,
            phantom: PhantomData,
        })
    }

    pub fn raw(&'s self) -> (&'s [u8], usize) {
        (self.mem, self.pitch)
    }

    pub fn line(&'s self, y: usize) -> GfxLine<'s, C> {
        GfxLine {
            mem: &self.mem[y * self.pitch..][..self.width * C::U::SIZE],
            phantom: PhantomData,
        }
    }
}

impl<'a: 's, 's, C: Color + Sized> GfxBufferMut<'a, C> {
    pub fn new(
        mem: &'a mut [u8],
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Option<GfxBufferMut<'a, C>> {
        if pitch < width * C::U::SIZE {
            return None;
        }
        if mem.len() < height * pitch {
            return None;
        }
        Some(Self {
            mem: &mut mem[..height * pitch],
            width,
            pitch,
            phantom: PhantomData,
        })
    }

    pub fn raw(&'s mut self) -> (&'s mut [u8], usize) {
        (self.mem, self.pitch)
    }

    pub fn line(&'s mut self, y: usize) -> GfxLineMut<'s, C> {
        GfxLineMut {
            mem: &mut self.mem[y * self.pitch..][..self.width * C::U::SIZE],
            phantom: PhantomData,
        }
    }

    pub fn lines(&'s mut self, y1: usize, y2: usize) -> (GfxLineMut<'s, C>, GfxLineMut<'s, C>) {
        let (mem1, mem2) = self.mem.split_at_mut(y2 * self.pitch);

        (
            GfxLineMut {
                mem: &mut mem1[y1 * self.pitch..][..self.width * C::U::SIZE],
                phantom: PhantomData,
            },
            GfxLineMut {
                mem: &mut mem2[..self.width * C::U::SIZE],
                phantom: PhantomData,
            },
        )
    }
}

impl<'a, C: Color + Sized> GfxLine<'a, C> {
    pub fn get(&self, x: usize) -> C {
        C::from_bits(C::U::endian_read_from::<LittleEndian>(
            &self.mem[x * C::U::SIZE..],
        ))
    }
}

impl<'a, C: Color + Sized> GfxLineMut<'a, C> {
    pub fn get(&self, x: usize) -> C {
        C::from_bits(C::U::endian_read_from::<LittleEndian>(
            &self.mem[x * C::U::SIZE..],
        ))
    }
    pub fn set(&mut self, x: usize, c: C) {
        C::U::endian_write_to::<LittleEndian>(&mut self.mem[x * C::U::SIZE..], c.to_bits());
    }
}

impl<C: Color + Sized> OwnedGfxBuffer<C> {
    pub fn new(width: usize, height: usize) -> OwnedGfxBuffer<C> {
        let mut v = Vec::new();
        v.resize(width * height * C::U::SIZE, 0);
        OwnedGfxBuffer {
            mem: v,
            width,
            height,
            phantom: PhantomData,
        }
    }

    pub fn buf<'a>(&'a self) -> GfxBuffer<'a, C> {
        GfxBuffer::new(&self.mem, self.width, self.height, self.width * C::U::SIZE).unwrap()
    }

    pub fn buf_mut<'a>(&'a mut self) -> GfxBufferMut<'a, C> {
        GfxBufferMut::new(
            &mut self.mem,
            self.width,
            self.height,
            self.width * C::U::SIZE,
        ).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color() {
        assert_eq!(Rgb565::new(0x10, 0x10, 0x10).is_some(), true);
        assert_eq!(Rgb565::new(0x1F, 0x3F, 0x1F).is_some(), true);
        assert_eq!(Rgb565::new(0x1F, 0x40, 0x1F).is_some(), false);

        let c1 = Rgb888::new_clamped(0xAA, 0x77, 0x33);
        let c2: Rgb555 = c1.into();
        assert_eq!(Rgb555::new(0xAA >> 3, 0x77 >> 3, 0x33 >> 3).unwrap(), c2);

        let c1 = Rgb565::new_clamped(0x13, 0x24, 0x14);
        let c2: Rgb888 = c1.into();
        assert_eq!(
            Rgb888::new(
                (0x13 << 3) | (0x13 >> 2),
                (0x24 << 2) | (0x24 >> 4),
                (0x14 << 3) | (0x14 >> 2)
            ).unwrap(),
            c2
        );
    }

    #[test]
    fn buffer() {
        let mut v1 = Vec::<u8>::new();
        let mut v2 = Vec::<u8>::new();
        v1.resize(128 * 128 * 2, 0);
        v2.resize(128 * 128 * 4, 0);

        assert_eq!(
            GfxBuffer::<Rgb888>::new(&mut v1, 128, 128, 256).is_some(),
            false
        );
        assert_eq!(
            GfxBuffer::<Rgb565>::new(&mut v1, 128, 128, 256).is_some(),
            true
        );

        {
            let mut buf1 = GfxBufferMut::<Rgb565>::new(&mut v1, 128, 128, 256).unwrap();
            let c1 = Rgb565::new_clamped(0x13, 0x24, 0x14);
            for y in 0..128 {
                let mut line = buf1.line(y);
                for x in 0..128 {
                    line.set(x, c1);
                }
            }
        }
        {
            let buf1 = GfxBuffer::<Rgb565>::new(&v1, 128, 128, 256).unwrap();
            let mut buf2 = GfxBufferMut::<Rgb888>::new(&mut v2, 128, 128, 512).unwrap();

            for y in 0..128 {
                let src = buf1.line(y);
                let mut dst = buf2.line(y);
                for x in 0..128 {
                    dst.set(x, src.get(x).into());
                }
            }
        }

        assert_eq!(
            LittleEndian::read_u32(&v2[0x1000..]),
            ((0x13 << 3) | (0x13 >> 2))
                | (((0x24 << 2) | (0x24 >> 4)) << 8)
                | (((0x14 << 3) | (0x14 >> 2)) << 16)
        );
    }
}
