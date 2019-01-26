use std::arch::x86_64::*;

#[inline]
#[target_feature(enable = "sse2")]
unsafe fn vselect(mask: __m128i, a: __m128i, b: __m128i) -> __m128i {
    _mm_or_si128(_mm_and_si128(mask, a), _mm_andnot_si128(mask, b))
}

#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn vch(
    vs: __m128i,
    vt: __m128i,
) -> (__m128i, __m128i, __m128i, __m128i, __m128i, __m128i) {
    #[allow(overflowing_literals)]
    let vones = _mm_set1_epi16(0xFFFF);
    let vzero = _mm_setzero_si128();

    let sign = _mm_srai_epi16(_mm_xor_si128(vs, vt), 15);
    let notsign = _mm_xor_si128(vones, sign);

    // GE is computed as follows:
    //   SIGN=-1 => VT < 0
    //   SIGN=0  => VS-VT >= 0
    //
    // Optimize as:
    //   VT - (VS &~ SIGN) + ~SIGN < 0
    // (with saturation on last addition to avoid overflow)
    let ge = _mm_srai_epi16(
        _mm_adds_epi16(notsign, _mm_sub_epi16(vt, _mm_andnot_si128(sign, vs))),
        15,
    );

    // LE is computed as follows:
    //   SIGN=-1 => VS+VT <= 0
    //   SIGN=0  => VT < 0
    //
    // Optimize as:
    //   (VS & SIGN) + VT + SIGN < 0
    // (with saturation on last addition to avoid overflow)
    let le = _mm_srai_epi16(
        _mm_adds_epi16(sign, _mm_add_epi16(_mm_and_si128(sign, vs), vt)),
        15,
    );

    // VCE is computed as follows:
    //  SIGN=-1 => VS+VT = -1
    //  SIGN=0  => 0
    //
    // Optimize as:
    //  ((VS + VT) == SIGN) & SIGN
    let vce = _mm_and_si128(sign, _mm_cmpeq_epi16(sign, _mm_add_epi16(vs, vt)));

    // NE is computed as follows:
    //  SIGN=-1 => VS+VT != 0 && VS+VT != -1
    //  SIGN=0  => VS-VT != 0
    //
    // Optimize as:
    //  SUM = VS^SIGN - VT
    //  !(SUM == 0 || SUM == SIGN)
    let add = _mm_sub_epi16(_mm_xor_si128(vs, sign), vt);
    let ne = _mm_xor_si128(
        vones,
        _mm_or_si128(_mm_cmpeq_epi16(add, vzero), _mm_cmpeq_epi16(add, sign)),
    );

    let res = vselect(
        sign,
        vselect(le, _mm_mullo_epi16(vones, vt), vs),
        vselect(ge, vt, vs),
    );

    (res, sign, ne, le, ge, vce)
}

#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn vcr(
    vs: __m128i,
    vt: __m128i,
) -> (__m128i, __m128i, __m128i, __m128i, __m128i, __m128i) {
    #[allow(overflowing_literals)]
    let vones = _mm_set1_epi16(0xFFFF);
    let vzero = _mm_setzero_si128();

    let sign = _mm_srai_epi16(_mm_xor_si128(vs, vt), 15);
    let notsign = _mm_xor_si128(vones, sign);

    // GE is computed as follows:
    //   SIGN=-1 => VT < 0
    //   SIGN=0  => VS-VT >= 0
    //
    // Optimize as:
    //   VT - (VS &~ SIGN) + ~SIGN < 0
    // (with saturation on last addition to avoid overflow)
    let ge = _mm_srai_epi16(
        _mm_adds_epi16(notsign, _mm_sub_epi16(vt, _mm_andnot_si128(sign, vs))),
        15,
    );

    // LE is computed as follows:
    //   SIGN=-1 => VS+VT+1 <= 0
    //   SIGN=0  => VT < 0
    //
    // Optimize as:
    //   (VS & SIGN) + VT < 0
    // (with saturation on last addition to avoid overflow)

    // FIXME: missing test! MUST REMOVE ADDS(SIGN)
    let le = _mm_srai_epi16(_mm_add_epi16(_mm_and_si128(sign, vs), vt), 15);

    let res = vselect(
        sign,
        vselect(le, _mm_mullo_epi16(vones, vt), vs),
        vselect(ge, vt, vs),
    );

    (res, vzero, vzero, le, ge, vzero)
}

#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn vcl(
    vs: __m128i,
    vt: __m128i,
    old_sign: __m128i,
    old_ne: __m128i,
    old_le: __m128i,
    old_ge: __m128i,
    old_vce: __m128i,
) -> (__m128i, __m128i, __m128i, __m128i, __m128i, __m128i) {
    let vzero = _mm_setzero_si128();

    // VTSIGN = SIGN ? -VT : VT
    let vtsign = _mm_sub_epi16(_mm_xor_si128(old_sign, vt), old_sign);

    // DI = SIGN ? VS-VT : VS+VT = VS - VTSIGN
    let di = _mm_sub_epi16(vs, vtsign);

    // IF SIGN
    let ncarry = _mm_cmpeq_epi16(di, _mm_adds_epu16(vt, vs));
    let di_zero = _mm_cmpeq_epi16(di, vzero);

    let le = vselect(
        old_vce,
        _mm_or_si128(di_zero, ncarry),
        _mm_and_si128(di_zero, ncarry),
    );
    let le = vselect(old_ne, old_le, le);

    // IF NOT SIGN
    let ge = _mm_cmpeq_epi16(_mm_subs_epu16(vt, vs), vzero);
    let ge = vselect(old_ne, old_ge, ge);

    // Select mask: MASK = SIGN ? LE : GE
    // Result: RES = MASK ? VT_SIGN : VS
    let res = vselect(vselect(old_sign, le, ge), vtsign, vs);

    (
        res,
        vzero,
        vzero,
        vselect(old_sign, le, old_le),
        vselect(old_sign, old_ge, ge),
        vzero,
    )
}
