extern crate byteorder;
extern crate emu;
extern crate num;
use self::byteorder::{BigEndian, ByteOrder, LittleEndian};
use self::emu::fp::formats::*;
use self::emu::fp::FixedPoint;
use self::emu::gfx::*;
use self::num::ToPrimitive;
use std::marker::PhantomData;

#[inline(always)]
fn int_draw_rect<'a, 'b, CF1, CF2, FP1, FP2, O1, O2>(
    dst: &mut GfxBufferMut<'a, CF1, O1>,
    dr: Rect<FP1>,
    src: &GfxBuffer<'b, CF2, O2>,
    st: Point<FP2>,
    dsdt: Point<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
    O1: ByteOrder,
    O2: ByteOrder,
{
    let dr = dr.truncate();
    let sx = st.x;
    let mut sy = st.y;

    let w = (dr.c1.x.floor() - dr.c0.x.floor()).to_usize().unwrap();
    if (w + 1) % 4 != 0 {
        println!("{:?}", w + 1);
        panic!("cannot unroll loop");
    }

    for dy in dr.c0.y.floor()..=dr.c1.y.floor() {
        let mut dst = dst.line(dy.to_usize().unwrap());
        let src = src.line(sy.floor().to_usize().unwrap());

        // FIXME: Do 4 pixels at a time (manual unroll). Not sure if it's OK.
        let mut sx = sx;
        for dx in (dr.c0.x.floor()..=dr.c1.x.floor()).step_by(4) {
            let c1 = src.get(sx.floor().to_usize().unwrap());
            sx = sx + dsdt.x;

            let c2 = src.get(sx.floor().to_usize().unwrap());
            sx = sx + dsdt.x;

            let c3 = src.get(sx.floor().to_usize().unwrap());
            sx = sx + dsdt.x;

            let c4 = src.get(sx.floor().to_usize().unwrap());
            sx = sx + dsdt.x;

            let didx = dx.to_usize().unwrap();
            dst.set4(didx, c1.cconv(), c2.cconv(), c3.cconv(), c4.cconv());
        }

        sy = sy + dsdt.y;
    }
}

pub fn draw_rect<'a, 'b, CF1, CF2, FP1, FP2, O1, O2>(
    dst: &mut GfxBufferMut<'a, CF1, O1>,
    dp: Point<FP1>,
    src: &GfxBuffer<'b, CF2, O2>,
    sr: Rect<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
    O1: ByteOrder,
    O2: ByteOrder,
{
    let dp = dp.truncate();
    let dr = Rect::new(dp, dp + Point::new(sr.width().cast(), sr.height().cast()));
    let sr = sr.truncate();
    let dsdt = Point::from_int(1, 1);
    int_draw_rect(dst, dr, src, sr.c0, dsdt);
}

pub fn draw_rect_scaled<'a, 'b, CF1, CF2, FP1, FP2, O1, O2>(
    dst: &mut GfxBufferMut<'a, CF1, O1>,
    dr: Rect<FP1>,
    src: &GfxBuffer<'b, CF2, O2>,
    sr: Rect<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
    O1: ByteOrder,
    O2: ByteOrder,
{
    let dsdx = (sr.width() + 1) / (dr.width() + 1);
    let dsdy = (sr.height() + 1) / (dr.height() + 1);
    let dsdt = Point::new(dsdx, dsdy);
    int_draw_rect(dst, dr, src, sr.c0, dsdt);
}

#[derive(Copy, Clone, Debug)]
pub enum DpColorFormat {
    RGBA,
    YUV,
    COLOR_INDEX,
    INTENSITY_ALPHA,
    INTENSITY,
}

impl DpColorFormat {
    pub fn from_bits(bits: usize) -> Option<DpColorFormat> {
        match bits {
            0 => Some(DpColorFormat::RGBA),
            1 => Some(DpColorFormat::YUV),
            2 => Some(DpColorFormat::COLOR_INDEX),
            3 => Some(DpColorFormat::INTENSITY_ALPHA),
            4 => Some(DpColorFormat::INTENSITY),
            _ => None,
        }
    }
}

pub struct RenderState<FPXY, FPST> {
    pub dst_cf: DpColorFormat,
    pub dst_bpp: usize,
    pub src_cf: DpColorFormat,
    pub src_bpp: usize,
    pub phantom: PhantomData<(FPXY, FPST)>,
}

pub type DpRenderState = RenderState<U30F2, I22F10>;

impl<FPXY: FixedPoint, FPST: FixedPoint> RenderState<FPXY, FPST> {
    #[inline]
    fn draw_rect_slopes2<CF1: ColorFormat, CF2: ColorFormat, O: ByteOrder>(
        &self,
        dst: (&mut [u8], usize, usize, usize),
        dr: Rect<FPXY>,
        src: (&[u8], usize, usize, usize),
        st: Point<FPST>,
        dsdt: Point<FPST>,
    ) {
        let mut dst = GfxBufferMut::<CF1, LittleEndian>::new(dst.0, dst.1, dst.2, dst.3).unwrap();
        let src = GfxBuffer::<CF2, O>::new(src.0, src.1, src.2, src.3).unwrap();
        int_draw_rect(&mut dst, dr, &src, st, dsdt);
    }

    #[inline]
    fn draw_rect_slopes1<CF1: ColorFormat>(
        &self,
        dst: (&mut [u8], usize, usize, usize),
        dr: Rect<FPXY>,
        src: (&[u8], usize, usize, usize),
        st: Point<FPST>,
        dsdt: Point<FPST>,
    ) {
        match self.src_cf {
            DpColorFormat::INTENSITY if self.src_bpp == 4 => {
                self.draw_rect_slopes2::<CF1, I4, BigEndian>(dst, dr, src, st, dsdt)
            }
            DpColorFormat::INTENSITY if self.src_bpp == 8 => {
                self.draw_rect_slopes2::<CF1, I8, BigEndian>(dst, dr, src, st, dsdt)
            }
            _ => panic!(
                "unimplemented src color format: {:?}/{}",
                self.src_cf, self.src_bpp
            ),
        }
    }

    pub fn draw_rect_slopes(
        &self,
        dst: (&mut [u8], usize, usize, usize),
        dr: Rect<FPXY>,
        src: (&[u8], usize, usize, usize),
        st: Point<FPST>,
        dsdt: Point<FPST>,
    ) {
        match self.dst_cf {
            DpColorFormat::RGBA if self.dst_bpp == 32 => {
                self.draw_rect_slopes1::<Rgb888>(dst, dr, src, st, dsdt)
            }
            _ => panic!(
                "unimplemented dst color format: {:?}/{}",
                self.dst_cf, self.dst_bpp
            ),
        }
    }
}
