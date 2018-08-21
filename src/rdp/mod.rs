extern crate byteorder;
extern crate emu;
use byteorder::{ByteOrder, LittleEndian};
use emu::gfx::{Color, ColorConverter, ColorFormat, Rgba8888};
use packed_simd::*;
use std::arch::x86_64::*;

type MultiColor = u16x8;

pub(crate) trait MColor: Sized + Copy {
    fn from_color<CF: ColorFormat>(c: Color<CF>) -> Self;
    fn get_color<CF: ColorFormat>(&self, idx: usize) -> Color<CF>;
    fn map_alpha(self, f: fn(u16) -> u16) -> Self;
    fn replace_alpha(self, alpha: Self) -> Self;
    fn replicate_alpha(self) -> Self;
    fn overflown(self) -> bool;
}

impl MColor for MultiColor {
    fn from_color<CF: ColorFormat>(c: Color<CF>) -> Self {
        let (r, g, b, a) = c.components();
        u16x8::new(
            r as u16, g as u16, b as u16, a as u16, r as u16, g as u16, b as u16, a as u16,
        )
    }

    fn overflown(self) -> bool {
        (self & MultiColor::splat(0xFF)) != self
    }

    fn get_color<CF: ColorFormat>(&self, idx: usize) -> Color<CF> {
        // Rust does not expose a _mm_pack* functions through the uAAxBB SIMD
        // structs, so there is no way to convert from u16x8 to u8x16 without
        // using scalar code. The following code is able to keep it fully
        // vectorized, and generate a final "MOVD XMM" instruction to
        // extract the required color index.
        let c = unsafe {
            let c = __m128i::from_bits(*self);
            let c = _mm_packus_epi16(c, _mm_setzero_si128());
            u8x16::from_bits(c)
        };
        let mut cbuf: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        c.write_to_slice_unaligned(&mut cbuf);
        match idx {
            0 => Color::<Rgba8888>::from_bits(LittleEndian::read_u32(&cbuf[0..4])).cconv(),
            1 => Color::<Rgba8888>::from_bits(LittleEndian::read_u32(&cbuf[4..8])).cconv(),
            _ => panic!("invalid MultiColor index"),
        }
    }

    fn map_alpha(self, f: fn(u16) -> u16) -> Self {
        let a1 = self.extract(3);
        let a2 = self.extract(7);
        self.replace(3, f(a1)).replace(7, f(a2))
    }
    fn replace_alpha(self, alpha: Self) -> Self {
        self.replace(3, alpha.extract(3))
            .replace(7, alpha.extract(7))
    }
    fn replicate_alpha(self) -> Self {
        let a1 = self.extract(3);
        let a2 = self.extract(7);
        self.replace(0, a1)
            .replace(1, a1)
            .replace(2, a1)
            .replace(4, a2)
            .replace(5, a2)
            .replace(6, a2)
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum CycleMode {
    One,
    Two,
    Copy,
    Fill,
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum DpColorFormat {
    Rgba,
    Yuv,
    ColorIndex,
    IntensityAlpha,
    Intensity,
}

impl Default for DpColorFormat {
    fn default() -> DpColorFormat {
        DpColorFormat::Rgba
    }
}

impl DpColorFormat {
    pub fn from_bits(bits: usize) -> Option<DpColorFormat> {
        match bits {
            0 => Some(DpColorFormat::Rgba),
            1 => Some(DpColorFormat::Yuv),
            2 => Some(DpColorFormat::ColorIndex),
            3 => Some(DpColorFormat::IntensityAlpha),
            4 => Some(DpColorFormat::Intensity),
            _ => None,
        }
    }
}

mod bl;
mod cc;
mod pipeline;
mod raster;
mod rdp;

pub use self::pipeline::PixelPipeline;
pub use self::rdp::Rdp;
