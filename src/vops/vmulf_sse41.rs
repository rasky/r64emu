use std::arch::x86_64::*;

#[inline(always)]
unsafe fn _mm_adds_epi32(a: __m128i, b: __m128i) -> __m128i {
    #[allow(overflowing_literals)]
    let int_min = _mm_set1_epi32(0x80000000);
    let int_max = _mm_set1_epi32(0x7FFFFFFF);

    let res = _mm_add_epi32(a, b);

    let sign_and = _mm_and_si128(a, b);

    let sign_or = _mm_or_si128(a, b);

    let min_sat_mask = _mm_andnot_si128(res, sign_and);

    let max_sat_mask = _mm_andnot_si128(sign_or, res);

    let res_temp = _mm_blendv_ps(
        _mm_castsi128_ps(res),
        _mm_castsi128_ps(int_min),
        _mm_castsi128_ps(min_sat_mask),
    );

    return _mm_castps_si128(_mm_blendv_ps(
        res_temp,
        _mm_castsi128_ps(int_max),
        _mm_castsi128_ps(max_sat_mask),
    ));
}

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

    if mac {
        let prev_acc1 = acc1;
        let prev_acc2 = acc2;

        // Add the partial result to the current accumulator
        // Using 32-bit registers, we add MD and LO in one shot,
        // but there could be a carry-over that should be added to HI...
        acc1 = _mm_add_epi32(acc1, _mm_unpacklo_epi16(aclo, acmd));
        acc2 = _mm_add_epi32(acc2, _mm_unpackhi_epi16(aclo, acmd));

        // Compute the carry-over from the previous addition.
        let mask = _mm_set1_epi32(0x80000000);
        let carry2 = _mm_packus_epi32(
            _mm_srli_epi32(
                _mm_cmpgt_epi32(_mm_xor_si128(prev_acc1, mask), _mm_xor_si128(acc1, mask)),
                31,
            ),
            _mm_srli_epi32(
                _mm_cmpgt_epi32(_mm_xor_si128(prev_acc2, mask), _mm_xor_si128(acc2, mask)),
                31,
            ),
        );

        // Add the carry-over to the partial HI part
        phi = _mm_add_epi16(phi, carry2);

        // Now add the partial HI part to the current accumulator HI.
        phi = _mm_add_epi16(phi, achi); // maybe ADDS? FIXME: golden test
    }

    // Repack partial result into 16-bit registers
    let plo = _mm_packus_epi32(_mm_and_si128(acc1, klomask), _mm_and_si128(acc2, klomask));
    let pmd = _mm_packs_epi32(_mm_srai_epi32(acc1, 16), _mm_srai_epi32(acc2, 16));

    // The accumulator is now 48 effective bits: PLO, PMD and PHI.
    // The result is computed by saturating the upper 32 bits (so PMD and PHI)
    let res = if signed {
        // Signed saturation of ACCUM HI/MD 32bit -> RES 16bit
        _mm_packs_epi32(_mm_unpacklo_epi16(pmd, phi), _mm_unpackhi_epi16(pmd, phi))
    } else {
        // Unsigned saturation of ACCUM HI/MD 32bit -> RES 16bit
        //   * Negative values: 0
        //   * Positive values < 0x7FFF: kept as-is
        //   * Positive values >= 0x8000: 0xFFFF
        // See golden test "overflow" for vmulu

        // In non-MAC, phi is either 0 or 0xFFFF, so we can use simplify
        // the clamping.
        if !mac {
            let mut x = _mm_andnot_si128(phi, pmd); // PHI<0? X=0
            x = _mm_or_si128(x, _mm_srai_epi16(x, 15)); // X>7FFF? X=FFFF
            x
        } else {
            let mut x = pmd;
            x = _mm_andnot_si128(_mm_cmpgt_epi16(kzero, phi), pmd); // PHI<0? X=0
            x = _mm_or_si128(_mm_cmpgt_epi16(phi, kzero), x); // PHI>0? X=FFFF
            x = _mm_or_si128(x, _mm_srai_epi16(x, 15)); // X>0x7FFF? X=FFFF
            x
        }
    };

    (res, plo, pmd, phi)
}

gen_mul_variant!(vmulf, internal_vmulfu, "sse4.1", true, false);
gen_mul_variant!(vmulu, internal_vmulfu, "sse4.1", false, false);
gen_mul_variant!(vmacf, internal_vmulfu, "sse4.1", true, true);
gen_mul_variant!(vmacu, internal_vmulfu, "sse4.1", false, true);
