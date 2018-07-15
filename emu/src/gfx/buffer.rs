extern crate byteorder;
extern crate typenum;

use self::byteorder::LittleEndian;
#[allow(unused_imports)]
use self::typenum::{
    U0, U1, U10, U11, U12, U13, U14, U15, U16, U17, U18, U19, U2, U20, U21, U22, U23, U24, U25,
    U26, U27, U28, U29, U3, U30, U31, U4, U5, U6, U7, U8, U9, Unsigned,
};
use super::super::bus::MemInt;
use super::{Color, ColorFormat, Rgb888};
use std::marker::PhantomData;

pub struct GfxBuffer<'a, CF: ColorFormat + Sized> {
    mem: &'a [u8],
    width: usize,
    pitch: usize,
    phantom: PhantomData<CF>,
}

pub struct GfxBufferMut<'a, CF: ColorFormat + Sized> {
    mem: &'a mut [u8],
    width: usize,
    pitch: usize,
    phantom: PhantomData<CF>,
}

pub struct GfxLine<'a, CF: ColorFormat + Sized> {
    mem: &'a [u8],
    phantom: PhantomData<CF>,
}

pub struct GfxLineMut<'a, CF: ColorFormat + Sized> {
    mem: &'a mut [u8],
    phantom: PhantomData<CF>,
}

pub struct OwnedGfxBuffer<CF: ColorFormat + Sized> {
    mem: Vec<u8>,
    width: usize,
    height: usize,
    phantom: PhantomData<CF>,
}

impl<'a: 's, 's, CF: ColorFormat + Sized> GfxBuffer<'a, CF> {
    pub fn new(
        mem: &'a [u8],
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Result<GfxBuffer<'a, CF>, String> {
        if pitch < width * CF::U::SIZE {
            return Err(format!(
                "pitch ({}) too small for buffer (width: {}, bpp: {})",
                pitch,
                width,
                CF::U::SIZE
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
            pitch,
            phantom: PhantomData,
        })
    }

    pub fn raw(&'s self) -> (&'s [u8], usize) {
        (self.mem, self.pitch)
    }

    pub fn line(&'s self, y: usize) -> GfxLine<'s, CF> {
        GfxLine {
            mem: &self.mem[y * self.pitch..][..self.width * CF::U::SIZE],
            phantom: PhantomData,
        }
    }
}

impl<'a: 's, 's, CF: ColorFormat + Sized> GfxBufferMut<'a, CF> {
    pub fn new(
        mem: &'a mut [u8],
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Result<GfxBufferMut<'a, CF>, String> {
        if pitch < width * CF::U::SIZE {
            return Err(format!(
                "pitch ({}) too small for buffer (width: {}, bpp: {})",
                pitch,
                width,
                CF::U::SIZE
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
            pitch,
            phantom: PhantomData,
        })
    }

    pub fn raw(&'s mut self) -> (&'s mut [u8], usize) {
        (self.mem, self.pitch)
    }

    pub fn line(&'s mut self, y: usize) -> GfxLineMut<'s, CF> {
        GfxLineMut {
            mem: &mut self.mem[y * self.pitch..][..self.width * CF::U::SIZE],
            phantom: PhantomData,
        }
    }

    pub fn lines(&'s mut self, y1: usize, y2: usize) -> (GfxLineMut<'s, CF>, GfxLineMut<'s, CF>) {
        let (mem1, mem2) = self.mem.split_at_mut(y2 * self.pitch);

        (
            GfxLineMut {
                mem: &mut mem1[y1 * self.pitch..][..self.width * CF::U::SIZE],
                phantom: PhantomData,
            },
            GfxLineMut {
                mem: &mut mem2[..self.width * CF::U::SIZE],
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

impl<'a, CF> BufferLineGetter<CF> for GfxLine<'a, CF>
where
    CF: ColorFormat + Sized,
{
    #[inline(always)]
    default fn get(&self, x: usize) -> Color<CF> {
        Color::from_bits(CF::U::endian_read_from::<LittleEndian>(
            &self.mem[x * CF::U::SIZE..],
        ))
    }
}

impl<'a, CF> BufferLineGetter<CF> for GfxLine<'a, CF>
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

impl<'a, CF> BufferLineGetter<CF> for GfxLineMut<'a, CF>
where
    CF: ColorFormat + Sized,
{
    #[inline(always)]
    default fn get(&self, x: usize) -> Color<CF> {
        Color::from_bits(CF::U::endian_read_from::<LittleEndian>(
            &self.mem[x * CF::U::SIZE..],
        ))
    }
}

impl<'a, CF> BufferLineGetter<CF> for GfxLineMut<'a, CF>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn get(&self, x: usize) -> Color<CF> {
        let val = self.mem[x / 2] >> ((x & 1) * 4);
        Color::from_bits(CF::U::truncate_from(val.into()))
    }
}

impl<'a, CF> BufferLineSetter<CF> for GfxLineMut<'a, CF>
where
    CF: ColorFormat + Sized,
{
    #[inline(always)]
    default fn set(&mut self, x: usize, c: Color<CF>) {
        CF::U::endian_write_to::<LittleEndian>(&mut self.mem[x * CF::U::SIZE..], c.to_bits());
    }
}

impl<'a, CF> BufferLineSetter<CF> for GfxLineMut<'a, CF>
where
    CF: ColorFormat<BITS = U4> + Sized,
{
    #[inline(always)]
    fn set(&mut self, x: usize, c: Color<CF>) {
        let val = self.mem[x / 2];
        let mask = 0xF0 >> ((x & 1) * 4);
        let val = (val & mask) | (c.to_bits().into() << ((x & 1) * 4)) as u8;
        CF::U::endian_write_to::<LittleEndian>(
            &mut self.mem[x * CF::U::SIZE..],
            CF::U::truncate_from(val.into()),
        );
    }
    #[inline(always)]
    fn set2(&mut self, x: usize, c1: Color<CF>, c2: Color<CF>) {
        let val = c1.to_bits().into() | (c2.to_bits().into() << 4);
        CF::U::endian_write_to::<LittleEndian>(
            &mut self.mem[x * CF::U::SIZE..],
            CF::U::truncate_from(val),
        );
    }
}

impl<CF: ColorFormat + Sized> OwnedGfxBuffer<CF> {
    pub fn new(width: usize, height: usize) -> OwnedGfxBuffer<CF> {
        let mut v = Vec::new();
        v.resize(width * height * CF::U::SIZE, 0);
        OwnedGfxBuffer {
            mem: v,
            width,
            height,
            phantom: PhantomData,
        }
    }

    pub fn buf<'a>(&'a self) -> GfxBuffer<'a, CF> {
        GfxBuffer::new(&self.mem, self.width, self.height, self.width * CF::U::SIZE).unwrap()
    }

    pub fn buf_mut<'a>(&'a mut self) -> GfxBufferMut<'a, CF> {
        GfxBufferMut::new(
            &mut self.mem,
            self.width,
            self.height,
            self.width * CF::U::SIZE,
        ).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Rgb565, Rgb888};
    use super::byteorder::ByteOrder;
    use super::*;

    #[test]
    fn buffer() {
        let mut v1 = Vec::<u8>::new();
        let mut v2 = Vec::<u8>::new();
        v1.resize(128 * 128 * 2, 0);
        v2.resize(128 * 128 * 4, 0);

        assert_eq!(
            GfxBuffer::<Rgb888>::new(&mut v1, 128, 128, 256).is_ok(),
            false
        );
        assert_eq!(
            GfxBuffer::<Rgb565>::new(&mut v1, 128, 128, 256).is_ok(),
            true
        );

        {
            let mut buf1 = GfxBufferMut::<Rgb565>::new(&mut v1, 128, 128, 256).unwrap();
            let c1 = Color::<Rgb565>::new_clamped(0x13, 0x24, 0x14, 0);
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
