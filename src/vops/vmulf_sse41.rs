use super::{acc_add, acc_clamp_signed, acc_clamp_unsigned2};
use std::arch::x86_64::*;

// SSE 4.1 version
#[inline]
#[target_feature(enable = "sse4.1")]
unsafe fn internal_vmulfu(
    vs: __m128i,
    vt: __m128i,
    aclo: __m128i,
    acmd: __m128i,
    achi: __m128i,
    signed: bool,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    // Constants
    let kzero = _mm_setzero_si128();
    let klomask = _mm_set1_epi32(0xFFFF);

    // Compute V0*V1 (signed), lower and higher part
    let mlo = _mm_mullo_epi16(vs, vt);
    let mhi = _mm_mulhi_epi16(vs, vt);

    // Switch to 32-bit registers. This is easier for accumulation
    let mut acc1 = _mm_unpacklo_epi16(mlo, mhi);
    let mut acc2 = _mm_unpackhi_epi16(mlo, mhi);

    if !mac {
        // We need to compute (V0*V1)*2 + 0.5. Unfortunately, both *2 and +0.5
        // could create a carry over into the 33-bit and SSE instructions
        // are not good at this.
        // So, as a first trick, let's do (V0*V1 + 0.25)*2 instead, and
        // +0.25 cannot overflow given that V0*V1 is the result of a 16*16 signed
        // multiplication.
        acc1 = _mm_add_epi32(acc1, _mm_set1_epi32(0x4000));
        acc2 = _mm_add_epi32(acc2, _mm_set1_epi32(0x4000));
    }

    // We're about to multiply *2. Get the bit being shifted away: it's our
    // ACCUM HI partial result (after sign-extension).
    let mut phi = _mm_packs_epi32(_mm_srai_epi32(acc1, 31), _mm_srai_epi32(acc2, 31));

    // Multiply by two
    acc1 = _mm_slli_epi32(acc1, 1);
    acc2 = _mm_slli_epi32(acc2, 1);

    // Repack partial result into 16-bit registers
    let mut plo = _mm_packus_epi32(_mm_and_si128(acc1, klomask), _mm_and_si128(acc2, klomask));
    let mut pmd = _mm_packus_epi32(_mm_srli_epi32(acc1, 16), _mm_srli_epi32(acc2, 16));

    if mac {
        let (new_acc_lo, new_acc_md, new_acc_hi) = acc_add(aclo, acmd, achi, plo, pmd, phi);

        plo = new_acc_lo;
        pmd = new_acc_md;
        phi = new_acc_hi;
    }

    let res = if signed {
        acc_clamp_signed(pmd, phi)
    } else {
        acc_clamp_unsigned2(pmd, phi)
    };

    (res, plo, pmd, phi)
}

gen_mul_variant!(vmulf, internal_vmulfu, "sse4.1", true, false);
gen_mul_variant!(vmulu, internal_vmulfu, "sse4.1", false, false);
gen_mul_variant!(vmacf, internal_vmulfu, "sse4.1", true, true);
gen_mul_variant!(vmacu, internal_vmulfu, "sse4.1", false, true);
