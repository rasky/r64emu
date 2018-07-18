use std::simd::*;

pub struct Color(u8, u8, u8, u8);
type MultiColor = u16x8;

pub(crate) trait MColor {
    fn from_color(c: Color) -> Self;
    fn replace_alpha(self, alpha: Self) -> Self;
    fn replicate_alpha(self) -> Self;
}

impl MColor for MultiColor {
    fn from_color(c: Color) -> Self {
        u16x8::new(
            c.0 as u16, c.1 as u16, c.2 as u16, c.3 as u16, c.0 as u16, c.1 as u16, c.2 as u16,
            c.3 as u16,
        )
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

mod bl;
mod cc;
mod raster;

pub use self::raster::RdpRasterizer;
