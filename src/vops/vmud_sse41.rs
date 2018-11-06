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

#[inline]
#[target_feature(enable = "sse4.1")]
unsafe fn acc_add(
    acc1_lo: __m128i,
    acc1_md: __m128i,
    acc1_hi: __m128i,
    acc2_lo: __m128i,
    acc2_md: __m128i,
    acc2_hi: __m128i,
) -> (__m128i, __m128i, __m128i) {
    let mut res_lo = _mm_add_epi16(acc1_lo, acc2_lo);
    let mut res_md = _mm_add_epi16(acc1_md, acc2_md);
    let mut res_hi = _mm_add_epi16(acc1_hi, acc2_hi);

    let signbit = _mm_set1_epi16(0x8000);
    let carry_lo = _mm_srli_epi16(
        _mm_cmpgt_epi16(
            _mm_xor_si128(acc2_lo, signbit),
            _mm_xor_si128(res_lo, signbit),
        ),
        15,
    );
    res_md = _mm_add_epi16(res_md, carry_lo);

    let carry_md = _mm_srli_epi16(
        _mm_cmpgt_epi16(
            _mm_xor_si128(acc2_md, signbit),
            _mm_xor_si128(res_md, signbit),
        ),
        15,
    );
    res_hi = _mm_add_epi16(res_hi, carry_md);

    (res_lo, res_md, res_hi)
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn internal_vmudn(
    vs: __m128i,
    vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
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

    let mut res = acc_lo;

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

gen_mul_variant!(vmudn, internal_vmudn, "sse4.1", false);
gen_mul_variant!(vmadn, internal_vmudn, "sse4.1", true);

gen_mul_variant!(vmudh, internal_vmudh, "sse4.1", false);
gen_mul_variant!(vmadh, internal_vmudh, "sse4.1", true);
