extern crate byteorder;
extern crate typenum;

use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
#[allow(unused_imports)]
use self::typenum::{
    Unsigned, U0, U1, U10, U11, U12, U13, U14, U15, U16, U17, U18, U19, U2, U20, U21, U22, U23,
    U24, U25, U26, U27, U28, U29, U3, U30, U31, U4, U5, U6, U7, U8, U9,
};
use super::super::bus::MemInt;
use super::{Color, ColorConverter, ColorFormat};
use std::marker::PhantomData;

pub struct GfxBuffer<'a, CF: ColorFormat + Sized, O: ByteOrder> {
    mem: &'a [u8],
    width: usize,
    height: usize,
    pitch: usize,
    phantom: PhantomData<(CF, O)>,
}

pub struct GfxBufferMut<'a, CF: ColorFormat + Sized, O: ByteOrder> {
    mem: &'a mut [u8],
    width: usize,
    height: usize,
    pitch: usize,
    phantom: PhantomData<(CF, O)>,
}

pub struct GfxLine<'a, CF: ColorFormat + Sized, O: ByteOrder> {
    mem: &'a [u8],
    phantom: PhantomData<(CF, O)>,
}

pub struct GfxLineMut<'a, CF: ColorFormat + Sized, O: ByteOrder> {
    mem: &'a mut [u8],
    phantom: PhantomData<(CF, O)>,
}

pub struct OwnedGfxBuffer<CF: ColorFormat + Sized, O: ByteOrder> {
    mem: Vec<u8>,
    width: usize,
    height: usize,
    phantom: PhantomData<(CF, O)>,
}

impl<'a: 's, 's, CF: ColorFormat + Sized, O: ByteOrder> GfxBuffer<'a, CF, O> {
    pub fn new(
        mem: &'a [u8],
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Result<GfxBuffer<'a, CF, O>, String> {
        if pitch < width * CF::BITS::to_usize() / 8 {
            return Err(format!(
                "pitch ({}) too small for buffer (width: {}, bpp: {})",
                pitch,
                width,
                CF::BITS::to_usize(),
            ));
        }
        if mem.len() < height * pitch {
            return Err(format!(
                "mem slice size ({}) too small for buffer (height: {}, pitch: {})",
                mem.len(),
                height,
                pitch,
            ));
        }
        Ok(Self {
            mem: &mem[..height * pitch],
            width,
            height,
            pitch,
            phantom: PhantomData,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn raw(&'s self) -> (&'a [u8], usize) {
        (self.mem, self.pitch)
    }

    pub fn line(&'s self, y: usize) -> GfxLine<'a, CF, O> {
        GfxLine {
            mem: &self.mem[y * self.pitch..][..self.width * CF::BITS::to_usize() / 8],
            phantom: PhantomData,
        }
    }
}

impl<'a: 's, 's, CF: ColorFormat + Sized, O: ByteOrder> GfxBufferMut<'a, CF, O> {
    pub fn new(
        mem: &'a mut [u8],
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Result<GfxBufferMut<'a, CF, O>, String> {
        if pitch < width * CF::BITS::to_usize() / 8 {
            return Err(format!(
                "pitch ({}) too small for buffer (width: {}, bpp: {})",
                pitch,
                width,
                CF::BITS::to_usize(),
            ));
        }
        if mem.len() < height * pitch {
            return Err(format!(
                "mem slice size ({}) too small for buffer (height: {}, pitch: {})",
                mem.len(),
                height,
                pitch,
            ));
        }
        Ok(Self {
            mem: &mut mem[..height * pitch],
            width,
            height,
            pitch,
            phantom: PhantomData,
        })
    }

    pub fn raw(&'s mut self) -> (&'s mut [u8], usize) {
        (self.mem, self.pitch)
    }

    pub fn line(&'s mut self, y: usize) -> GfxLineMut<'s, CF, O> {
        GfxLineMut {
            mem: &mut self.mem[y * self.pitch..][..self.width * CF::BITS::to_usize() / 8],
            phantom: PhantomData,
        }
    }

    pub fn lines(
        &'s mut self,
        y1: usize,
        y2: usize,
    ) -> (GfxLineMut<'s, CF, O>, GfxLineMut<'s, CF, O>) {
        let (mem1, mem2) = self.mem.split_at_mut(y2 * self.pitch);

        (
            GfxLineMut {
                mem: &mut mem1[y1 * self.pitch..][..self.width * CF::BITS::to_usize() / 8],
                phantom: PhantomData,
            },
            GfxLineMut {
                mem: &mut mem2[..self.width * CF::BITS::to_usize() / 8],
                phantom: PhantomData,
            },
        )
    }
}

pub trait BufferLineGetter<CF: ColorFormat> {
    #[inline(always)]
    fn get(&self, x: usize) -> Color<CF>;
    #[inline(always)]
    fn get2(&self, x: usize) -> (Color<CF>, Color<CF>) {
        let c1 = self.get(x);
        let c2 = self.get(x + 1);
        (c1, c2)
    }
    #[inline(always)]
    fn get4(&self, x: usize) -> (Color<CF>, Color<CF>, Color<CF>, Color<CF>) {
        let (c1, c2) = self.get2(x);
        let (c3, c4) = self.get2(x + 2);
        (c1, c2, c3, c4)
    }
}

pub trait BufferLineSetter<CF: ColorFormat> {
    #[inline(always)]
    fn set(&mut self, x: usize, c: Color<CF>);
    #[inline(always)]
    fn set2(&mut self, x: usize, c1: Color<CF>, c2: Color<CF>) {
        self.set(x, c1);
        self.set(x + 1, c2);
    }
    #[inline(always)]
    fn set4(&mut self, x: usize, c1: Color<CF>, c2: Color<CF>, c3: Color<CF>, c4: Color<CF>) {
        self.set2(x, c1, c2);
        self.set2(x + 2, c3, c4);
    }
}

impl<'a, CF, O> BufferLineGetter<CF> for GfxLine<'a, CF, O>
where
    CF: ColorFormat + Sized,
    O: ByteOrder,
{
    #[inline(always)]
    default fn get(&self, x: usize) -> Color<CF> {
        Color::from_bits(CF::U::endian_read_from::<O>(
            &self.mem[x * CF::BITS::to_usize() / 8..],
        ))
    }
}

impl<'a, CF> BufferLineGetter<CF> for GfxLine<'a, CF, LittleEndian>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn get(&self, x: usize) -> Color<CF> {
        let val = self.mem[x / 2] >> ((x & 1) * 4);
        Color::from_bits(CF::U::truncate_from(val.into()))
    }
    #[inline(always)]
    fn get2(&self, x: usize) -> (Color<CF>, Color<CF>) {
        let val = self.mem[x / 2];
        (
            Color::from_bits(CF::U::truncate_from((val & 0xF).into())),
            Color::from_bits(CF::U::truncate_from((val >> 4).into())),
        )
    }
}

impl<'a, CF> BufferLineGetter<CF> for GfxLine<'a, CF, BigEndian>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn get(&self, x: usize) -> Color<CF> {
        let val = self.mem[x / 2] >> ((!x & 1) * 4);
        Color::from_bits(CF::U::truncate_from(val.into()))
    }
    #[inline(always)]
    fn get2(&self, x: usize) -> (Color<CF>, Color<CF>) {
        let val = self.mem[x / 2];
        (
            Color::from_bits(CF::U::truncate_from((val >> 4).into())),
            Color::from_bits(CF::U::truncate_from((val & 0xF).into())),
        )
    }
}

impl<'a, CF, O> BufferLineGetter<CF> for GfxLineMut<'a, CF, O>
where
    CF: ColorFormat + Sized,
    O: ByteOrder,
{
    #[inline(always)]
    default fn get(&self, x: usize) -> Color<CF> {
        Color::from_bits(CF::U::endian_read_from::<O>(
            &self.mem[x * CF::BITS::to_usize() / 8..],
        ))
    }
}

impl<'a, CF> BufferLineGetter<CF> for GfxLineMut<'a, CF, LittleEndian>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn get(&self, x: usize) -> Color<CF> {
        let val = self.mem[x / 2] >> ((x & 1) * 4);
        Color::from_bits(CF::U::truncate_from(val.into()))
    }
}

impl<'a, CF> BufferLineGetter<CF> for GfxLineMut<'a, CF, BigEndian>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn get(&self, x: usize) -> Color<CF> {
        let val = self.mem[x / 2] >> ((!x & 1) * 4);
        Color::from_bits(CF::U::truncate_from(val.into()))
    }
}

impl<'a, CF, O> BufferLineSetter<CF> for GfxLineMut<'a, CF, O>
where
    CF: ColorFormat + Sized,
    O: ByteOrder,
{
    #[inline(always)]
    default fn set(&mut self, x: usize, c: Color<CF>) {
        CF::U::endian_write_to::<O>(&mut self.mem[x * CF::BITS::to_usize() / 8..], c.to_bits());
    }
}

impl<'a, CF> BufferLineSetter<CF> for GfxLineMut<'a, CF, LittleEndian>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn set(&mut self, x: usize, c: Color<CF>) {
        let val = self.mem[x / 2];
        let mask = 0xF0 >> ((x & 1) * 4);
        let val = (val & mask) | (c.to_bits().into() << ((x & 1) * 4)) as u8;
        self.mem[x / 2] = val as u8;
    }
    #[inline(always)]
    fn set2(&mut self, x: usize, c1: Color<CF>, c2: Color<CF>) {
        let val = c1.to_bits().into() | (c2.to_bits().into() << 4);
        self.mem[x / 2] = val as u8;
    }
}

impl<'a, CF> BufferLineSetter<CF> for GfxLineMut<'a, CF, BigEndian>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn set(&mut self, x: usize, c: Color<CF>) {
        let val = self.mem[x / 2];
        let mask = 0xF0 >> ((!x & 1) * 4);
        let val = (val & mask) | (c.to_bits().into() << ((!x & 1) * 4)) as u8;
        self.mem[x / 2] = val;
    }
    #[inline(always)]
    fn set2(&mut self, x: usize, c1: Color<CF>, c2: Color<CF>) {
        let val = c2.to_bits().into() | (c1.to_bits().into() << 4);
        self.mem[x / 2] = val as u8;
    }
}

impl<CF: ColorFormat + Sized, O: ByteOrder> OwnedGfxBuffer<CF, O> {
    pub fn from_buf<CF2: ColorFormat + Sized, O2: ByteOrder>(
        buf: &GfxBuffer<CF2, O2>,
    ) -> OwnedGfxBuffer<CF, O> {
        let (w, h) = (buf.width, buf.height);
        let mut dst = OwnedGfxBuffer::<CF, O>::new(w, h);
        for y in 0..h {
            let src = buf.line(y);
            let mut dstbuf = dst.buf_mut();
            let mut dst = dstbuf.line(y);
            for x in 0..w {
                dst.set(x, src.get(x).cconv());
            }
        }
        dst
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn new(width: usize, height: usize) -> OwnedGfxBuffer<CF, O> {
        let mut v = Vec::new();
        v.resize(width * height * CF::BITS::to_usize() / 8, 0);
        OwnedGfxBuffer {
            mem: v,
            width,
            height,
            phantom: PhantomData,
        }
    }

    pub fn buf<'a>(&'a self) -> GfxBuffer<'a, CF, O> {
        GfxBuffer::new(
            &self.mem,
            self.width,
            self.height,
            self.width * CF::BITS::to_usize() / 8,
        )
        .unwrap()
    }

    pub fn buf_mut<'a>(&'a mut self) -> GfxBufferMut<'a, CF, O> {
        GfxBufferMut::new(
            &mut self.mem,
            self.width,
            self.height,
            self.width * CF::BITS::to_usize() / 8,
        )
        .unwrap()
    }
}

pub type GfxBufferLE<'a, CF> = GfxBuffer<'a, CF, LittleEndian>;
pub type GfxBufferBE<'a, CF> = GfxBuffer<'a, CF, BigEndian>;
pub type GfxBufferMutLE<'a, CF> = GfxBufferMut<'a, CF, LittleEndian>;
pub type GfxBufferMutBE<'a, CF> = GfxBufferMut<'a, CF, BigEndian>;
pub type OwnedGfxBufferLE<CF> = OwnedGfxBuffer<CF, LittleEndian>;
pub type OwnedGfxBufferBE<CF> = OwnedGfxBuffer<CF, BigEndian>;

#[cfg(test)]
mod tests {
    use super::super::{Abgr8888, ColorConverter, Rgb565, Rgb888, Rgba8888, I4};
    use super::byteorder::ByteOrder;
    use super::*;

    #[test]
    fn buffer() {
        let mut v1 = Vec::<u8>::new();
        let mut v2 = Vec::<u8>::new();
        v1.resize(128 * 128 * 2, 0);
        v2.resize(128 * 128 * 4, 0);

        assert_eq!(
            GfxBuffer::<Rgb888, LittleEndian>::new(&mut v1, 128, 128, 256).is_ok(),
            false
        );
        assert_eq!(
            GfxBuffer::<Rgb565, LittleEndian>::new(&mut v1, 128, 128, 256).is_ok(),
            true
        );

        {
            let mut buf1 =
                GfxBufferMut::<Rgb565, LittleEndian>::new(&mut v1, 128, 128, 256).unwrap();
            let c1 = Color::<Rgb565>::new_clamped(0x13, 0x24, 0x14, 0);
            for y in 0..128 {
                let mut line = buf1.line(y);
                for x in 0..128 {
                    line.set(x, c1);
                }
            }
        }
        {
            let buf1 = GfxBuffer::<Rgb565, LittleEndian>::new(&v1, 128, 128, 256).unwrap();
            let mut buf2 =
                GfxBufferMut::<Rgb888, LittleEndian>::new(&mut v2, 128, 128, 512).unwrap();

            for y in 0..128 {
                let src = buf1.line(y);
                let mut dst = buf2.line(y);
                for x in 0..128 {
                    dst.set(x, src.get(x).cconv());
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

    #[test]
    fn bpp4() {
        let mut v1 = Vec::<u8>::new();
        let mut v2 = Vec::<u8>::new();
        v1.resize(128 * 128 / 2, 0);
        v2.resize(128 * 128 / 2, 0);

        assert_eq!(
            GfxBuffer::<I4, LittleEndian>::new(&mut v1, 128, 128, 63).is_ok(),
            false
        );
        assert_eq!(
            GfxBuffer::<I4, LittleEndian>::new(&mut v1, 128, 128, 64).is_ok(),
            true
        );

        {
            let mut buf1 = GfxBufferMut::<I4, LittleEndian>::new(&mut v1, 128, 128, 64).unwrap();
            let c0 = Color::<I4>::new_clamped(0x2, 0, 0, 0);
            let c1 = Color::<I4>::new_clamped(0xA, 0, 0, 0);
            for y in 0..128 {
                let mut line = buf1.line(y);
                for x in 0..128 {
                    if x & 1 == 0 {
                        line.set(x, c0);
                    } else {
                        line.set(x, c1);
                    }
                }
            }
        }
        assert_eq!(v1[128], 0xA2);

        {
            let mut buf1 = GfxBufferMut::<I4, LittleEndian>::new(&mut v2, 128, 128, 64).unwrap();
            let buf2 = GfxBuffer::<I4, LittleEndian>::new(&mut v1, 128, 128, 64).unwrap();
            for y in 0..128 {
                let mut dst = buf1.line(y);
                let src = buf2.line(y);
                for x in (0..128).step_by(4) {
                    let (c1, c2, c3, c4) = src.get4(x);
                    dst.set4(x, c1, c3, c2, c4);
                }
            }
        }

        assert_eq!(v2[128], 0x22);
        assert_eq!(v2[129], 0xAA);
    }

    #[test]
    fn endian() {
        let mut v1 = Vec::<u8>::new();
        v1.resize(128 * 128 * 4, 0);

        let c = Color::new_clamped(0x12, 0x34, 0x56, 0x78);
        {
            let mut buf1 = GfxBufferMutBE::<Abgr8888>::new(&mut v1, 128, 128, 128 * 4).unwrap();
            buf1.line(4).set(4, c);
            assert_eq!(buf1.line(4).get(4), c);
        }
        {
            let buf1 = GfxBufferBE::<Abgr8888>::new(&mut v1, 128, 128, 128 * 4).unwrap();
            assert_eq!(buf1.line(4).get(4), c);
        }
        {
            let buf1 = GfxBufferLE::<Rgba8888>::new(&mut v1, 128, 128, 128 * 4).unwrap();
            assert_eq!(buf1.line(4).get(4), c.cconv());
        }
        {
            let buf1 = GfxBufferBE::<I4>::new(&mut v1, 128 * 8, 128, 128 * 4).unwrap();
            assert_eq!(buf1.line(4).get(32), Color::new_clamped(0x1, 0, 0, 0));
            assert_eq!(buf1.line(4).get(33), Color::new_clamped(0x2, 0, 0, 0));
            assert_eq!(buf1.line(4).get(36), Color::new_clamped(0x5, 0, 0, 0));
            assert_eq!(buf1.line(4).get(37), Color::new_clamped(0x6, 0, 0, 0));
            assert_eq!(
                buf1.line(4).get2(36),
                (
                    Color::new_clamped(0x5, 0, 0, 0),
                    Color::new_clamped(0x6, 0, 0, 0),
                )
            );
            assert_eq!(
                buf1.line(4).get4(36),
                (
                    Color::new_clamped(0x5, 0, 0, 0),
                    Color::new_clamped(0x6, 0, 0, 0),
                    Color::new_clamped(0x7, 0, 0, 0),
                    Color::new_clamped(0x8, 0, 0, 0),
                )
            );
        }
        {
            let buf1 = GfxBufferLE::<I4>::new(&mut v1, 128 * 8, 128, 128 * 4).unwrap();
            assert_eq!(buf1.line(4).get(32), Color::new_clamped(0x2, 0, 0, 0));
            assert_eq!(buf1.line(4).get(33), Color::new_clamped(0x1, 0, 0, 0));
            assert_eq!(buf1.line(4).get(36), Color::new_clamped(0x6, 0, 0, 0));
            assert_eq!(buf1.line(4).get(37), Color::new_clamped(0x5, 0, 0, 0));
            assert_eq!(
                buf1.line(4).get2(36),
                (
                    Color::new_clamped(0x6, 0, 0, 0),
                    Color::new_clamped(0x5, 0, 0, 0),
                )
            );
            assert_eq!(
                buf1.line(4).get4(36),
                (
                    Color::new_clamped(0x6, 0, 0, 0),
                    Color::new_clamped(0x5, 0, 0, 0),
                    Color::new_clamped(0x8, 0, 0, 0),
                    Color::new_clamped(0x7, 0, 0, 0),
                )
            );
        }
    }
}
