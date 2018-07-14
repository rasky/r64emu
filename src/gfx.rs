extern crate num;
use self::num::ToPrimitive;
use super::emu::fp::FixedPoint;
use super::emu::gfx::{ColorFormat, GfxBuffer, GfxBufferMut, Point, Rect};

#[inline(always)]
fn int_draw_rect<'a, 'b, CF1, CF2, FP1, FP2>(
    dst: &mut GfxBufferMut<'a, CF1>,
    dr: Rect<FP1>,
    src: &GfxBuffer<'b, CF2>,
    sr: Rect<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
{
    let dr = dr.truncate();
    let sdx = (sr.width() + 1) / (dr.width() + 1);
    let sdy = (sr.height() + 1) / (dr.height() + 1);
    let sx = sr.c0.x;
    let mut sy = sr.c0.y;

    for dy in dr.c0.y.floor()..=dr.c1.y.floor() {
        let mut dst = dst.line(dy.to_usize().unwrap());
        let src = src.line(sy.floor().to_usize().unwrap());

        let mut sx = sx;
        for dx in dr.c0.x.floor()..=dr.c1.x.floor() {
            let sidx = sx.floor().to_usize().unwrap();
            let didx = dx.to_usize().unwrap();

            let pixel = src.get(sidx);
            dst.set(didx, pixel.into());

            sx = sx + sdx;
        }

        sy = sy + sdy;
    }
}

pub fn draw_rect<'a, 'b, CF1, CF2, FP1, FP2>(
    dst: &mut GfxBufferMut<'a, CF1>,
    dp: Point<FP1>,
    src: &GfxBuffer<'b, CF2>,
    sr: Rect<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
{
    let dr = Rect::new(dp, dp + Point::new(sr.width().cast(), sr.height().cast()));
    int_draw_rect(dst, dr, src, sr);
}

pub fn draw_rect_scaled<'a, 'b, CF1, CF2, FP1, FP2>(
    dst: &mut GfxBufferMut<'a, CF1>,
    dr: Rect<FP1>,
    src: &GfxBuffer<'b, CF2>,
    sr: Rect<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
{
    int_draw_rect(dst, dr, src, sr);
}
