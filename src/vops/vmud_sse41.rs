use super::acc_add;
use std::arch::x86_64::*;

#[inline]
#[target_feature(enable = "sse4.1")]
unsafe fn acc_clamp_signed(acc_md: __m128i, acc_hi: __m128i) -> __m128i {
    _mm_packs_epi32(
        _mm_unpacklo_epi16(acc_md, acc_hi),
        _mm_unpackhi_epi16(acc_md, acc_hi),
    )
}

#[inline]
#[target_feature(enable = "sse4.1")]
unsafe fn acc_clamp_unsigned(mut x: __m128i, acc_md: __m128i, acc_hi: __m128i) -> __m128i {
    let min = _mm_set1_epi32(0xFFFF_8000);
    let max = _mm_set1_epi32(0x0000_7FFF);

    let acc1 = _mm_unpacklo_epi16(acc_md, acc_hi);
    let acc2 = _mm_unpackhi_epi16(acc_md, acc_hi);
    let mask_min = _mm_packs_epi32(_mm_cmpgt_epi32(min, acc1), _mm_cmpgt_epi32(min, acc2));
    let mask_max = _mm_packs_epi32(_mm_cmpgt_epi32(acc1, max), _mm_cmpgt_epi32(acc2, max));

    x = _mm_andnot_si128(mask_min, x); // <MIN? X=0
    x = _mm_or_si128(mask_max, x); // >MAX? X=FFFF
    x
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn internal_vmudnm(
    mut vs: __m128i,
    mut vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    mac: bool,
    mid: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    let (vs,vt) = if mid {(vt,vs)} else {(vs,vt)};

    let mut acc_lo = _mm_mullo_epi16(vs, vt);
    let mut acc_md = _mm_mulhi_epi16(vs, vt);

    let sign = _mm_srai_epi16(vs, 15);
    acc_md = _mm_add_epi16(acc_md, _mm_and_si128(vt, sign));
    let mut acc_hi = _mm_srai_epi16(acc_md, 15);

    if mac {
        let (new_acc_lo, new_acc_md, new_acc_hi) =
            acc_add(old_acc_lo, old_acc_md, old_acc_hi, acc_lo, acc_md, acc_hi);
        acc_lo = new_acc_lo;
        acc_md = new_acc_md;
        acc_hi = new_acc_hi;
    }

    let mut res = if mid {acc_md} else {acc_lo};

    // The clamping is always performed, but we can avoid doing it
    // in the non-mac case as acc_hi is always a sign-extension of acc_md,
    // so there's nothing to do.
    if mac {
        res = acc_clamp_unsigned(res, acc_md, acc_hi);
    }

    (res, acc_lo, acc_md, acc_hi)
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn internal_vmudh(
    vs: __m128i,
    vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    let mut acc_lo = _mm_setzero_si128();
    let mut acc_md = _mm_mullo_epi16(vs, vt);
    let mut acc_hi = _mm_mulhi_epi16(vs, vt);

    if mac {
        let (new_acc_lo, new_acc_md, new_acc_hi) =
            acc_add(old_acc_lo, old_acc_md, old_acc_hi, acc_lo, acc_md, acc_hi);
        acc_lo = new_acc_lo;
        acc_md = new_acc_md;
        acc_hi = new_acc_hi;
    }

    let res = acc_clamp_signed(acc_md, acc_hi);
    (res, acc_lo, acc_md, acc_hi)
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn internal_vmudl(
    vs: __m128i,
    vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    let mut acc_lo = _mm_mulhi_epi16(vs, vt);

    let toggle = _mm_set1_epi16(0x7FFF);
    acc_lo = _mm_add_epi16(acc_lo, _mm_and_si128(_mm_and_si128(vt, toggle), _mm_srai_epi16(vs, 15)));
    acc_lo = _mm_add_epi16(acc_lo, _mm_and_si128(_mm_and_si128(vs, toggle), _mm_srai_epi16(vt, 15)));

    let mut acc_md = _mm_setzero_si128();
    let mut acc_hi = _mm_setzero_si128();

    if mac {
        let (new_acc_lo, new_acc_md, new_acc_hi) =
            acc_add(old_acc_lo, old_acc_md, old_acc_hi, acc_lo, acc_md, acc_hi);
        acc_lo = new_acc_lo;
        acc_md = new_acc_md;
        acc_hi = new_acc_hi;
    }

    let mut res = acc_lo;

    // The clamping is always performed, but we can avoid doing it
    // in the non-mac case as the upper part is always zero.
    if mac {
        res = acc_clamp_unsigned(res, acc_md, acc_hi);
    }

    (res, acc_lo, acc_md, acc_hi)
}

gen_mul_variant!(vmudn, internal_vmudnm, "sse4.1", false, false);
gen_mul_variant!(vmadn, internal_vmudnm, "sse4.1", true, false);
gen_mul_variant!(vmudm, internal_vmudnm, "sse4.1", false, true);
gen_mul_variant!(vmadm, internal_vmudnm, "sse4.1", true, true);

gen_mul_variant!(vmudh, internal_vmudh, "sse4.1", false);
gen_mul_variant!(vmadh, internal_vmudh, "sse4.1", true);

gen_mul_variant!(vmudl, internal_vmudl, "sse4.1", false );
gen_mul_variant!(vmadl, internal_vmudl, "sse4.1", true);
