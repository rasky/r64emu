extern crate num;
use self::num::ToPrimitive;
use super::emu::fp::FixedPoint;
use super::emu::gfx::*;

#[inline(always)]
fn int_draw_rect<'a, 'b, CF1, CF2, FP1, FP2>(
    dst: &mut GfxBufferMut<'a, CF1>,
    dr: Rect<FP1>,
    src: &GfxBuffer<'b, CF2>,
    st: Point<FP2>,
    dsdt: Point<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
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
    let dp = dp.truncate();
    let dr = Rect::new(dp, dp + Point::new(sr.width().cast(), sr.height().cast()));
    let sr = sr.truncate();
    let dsdt = Point::from_int(1, 1);
    int_draw_rect(dst, dr, src, sr.c0, dsdt);
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
    let dsdx = (sr.width() + 1) / (dr.width() + 1);
    let dsdy = (sr.height() + 1) / (dr.height() + 1);
    let dsdt = Point::new(dsdx, dsdy);
    int_draw_rect(dst, dr, src, sr.c0, dsdt);
}

pub fn draw_rect_slopes<'a, 'b, CF1, CF2, FP1, FP2>(
    dst: &mut GfxBufferMut<'a, CF1>,
    dr: Rect<FP1>,
    src: &GfxBuffer<'b, CF2>,
    st: Point<FP2>,
    dsdt: Point<FP2>,
) where
    CF1: ColorFormat,
    CF2: ColorFormat,
    FP1: FixedPoint,
    FP2: FixedPoint,
{
    int_draw_rect(dst, dr, src, st, dsdt);
}
