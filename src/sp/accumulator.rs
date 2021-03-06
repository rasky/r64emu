use std::arch::x86_64::*;

#[inline]
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn acc_add(
    acc1_lo: __m128i,
    acc1_md: __m128i,
    acc1_hi: __m128i,
    acc2_lo: __m128i,
    acc2_md: __m128i,
    acc2_hi: __m128i,
) -> (__m128i, __m128i, __m128i) {
    // Add the three parts to produce partial results (without carry).
    let res_lo = _mm_add_epi16(acc1_lo, acc2_lo);
    let mut res_md = _mm_add_epi16(acc1_md, acc2_md);
    let mut res_hi = _mm_add_epi16(acc1_hi, acc2_hi);

    // Check whether there was a carry generated while adding
    // the low and mid part. Carry is materialized as 0xFFFF (-1).
    #[allow(overflowing_literals)]
    let signbit = _mm_set1_epi16(0x8000);
    let carry_lo = _mm_cmpgt_epi16(
        _mm_xor_si128(acc2_lo, signbit),
        _mm_xor_si128(res_lo, signbit),
    );
    let carry_md = _mm_cmpgt_epi16(
        _mm_xor_si128(acc2_md, signbit),
        _mm_xor_si128(res_md, signbit),
    );

    // Tricky: check whether the carry from the low part (if any)
    // generates an overflow in the mid part. This only happens when
    // there is a carry (carry_lo=0xFFFF) and the midpart result is 0xFFFF.
    let carry_md2 = _mm_and_si128(carry_lo, _mm_cmpeq_epi16(res_md, carry_lo));

    // Add the three carries into the result. Since they are materialized
    // as -1, we use a subtraction to add them.
    res_md = _mm_sub_epi16(res_md, carry_lo);
    res_hi = _mm_sub_epi16(res_hi, carry_md);
    res_hi = _mm_sub_epi16(res_hi, carry_md2);

    (res_lo, res_md, res_hi)
}

#[inline]
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn acc_clamp_signed(acc_md: __m128i, acc_hi: __m128i) -> __m128i {
    _mm_packs_epi32(
        _mm_unpacklo_epi16(acc_md, acc_hi),
        _mm_unpackhi_epi16(acc_md, acc_hi),
    )
}

#[inline]
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn acc_clamp_unsigned3(
    mut x: __m128i,
    acc_md: __m128i,
    acc_hi: __m128i,
) -> __m128i {
    // Unsigned saturation of X given the current 32-bit MD/HI accumulator value.
    //   * Negative accum values: X=0
    //   * Positive accum values < 0x7FFF: X kept as-is
    //   * Positive accum values >= 0x8000: X=0xFFFF
    #[allow(overflowing_literals)]
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
pub(crate) unsafe fn acc_clamp_unsigned2(mut x: __m128i, acc_hi: __m128i) -> __m128i {
    // Same as acc_clamp_unsigned2, but with X==ACCUM_MD.
    // This allows us to skip a few operations.
    let kzero = _mm_setzero_si128();
    x = _mm_andnot_si128(_mm_cmpgt_epi16(kzero, acc_hi), x); // PHI<0? X=0
    x = _mm_or_si128(_mm_cmpgt_epi16(acc_hi, kzero), x); // PHI>0? X=FFFF
    x = _mm_or_si128(x, _mm_srai_epi16(x, 15)); // X>0x7FFF? X=FFFF
    x
}
