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
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse4.1")]
pub unsafe fn vmulf(
    vs: __m128i,
    vt: __m128i,
    aclo: __m128i,
    acmd: __m128i,
    _achi: __m128i,
    signed: bool,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    let vs1 = _mm_srai_epi32(_mm_unpacklo_epi16(vs, vs), 16);
    let vs2 = _mm_srai_epi32(_mm_unpackhi_epi16(vs, vs), 16);
    let vt1 = _mm_srai_epi32(_mm_unpacklo_epi16(vt, vt), 16);
    let vt2 = _mm_srai_epi32(_mm_unpackhi_epi16(vt, vt), 16);

    let mut acc1 = _mm_slli_epi32(_mm_mullo_epi32(vs1, vt1), 1);
    let mut acc2 = _mm_slli_epi32(_mm_mullo_epi32(vs2, vt2), 1);

    if mac {
        if signed {
            acc1 = _mm_adds_epi32(acc1, _mm_unpacklo_epi16(aclo, acmd));
            acc2 = _mm_adds_epi32(acc2, _mm_unpackhi_epi16(aclo, acmd));
        } else {
            acc1 = _mm_add_epi32(acc1, _mm_unpacklo_epi16(aclo, acmd));
            acc2 = _mm_add_epi32(acc2, _mm_unpackhi_epi16(aclo, acmd));
        }
    }

    let kzero = _mm_setzero_si128();
    let klomask = _mm_set1_epi32(0xFFFF);

    if !mac {
        acc1 = _mm_add_epi32(acc1, _mm_set1_epi32(0x8000));
        acc2 = _mm_add_epi32(acc2, _mm_set1_epi32(0x8000));
    }

    let res = _mm_packs_epi32(_mm_srai_epi32(acc1, 16), _mm_srai_epi32(acc2, 16));
    let plo = _mm_packus_epi32(_mm_and_si128(acc1, klomask), _mm_and_si128(acc2, klomask));
    let pmd = res;
    let phi = if mac && !signed {
        kzero
    } else {
        _mm_cmpgt_epi16(kzero, pmd)
    };

    let res = if signed {
        res
    } else {
        if mac {
            _mm_or_si128(_mm_cmplt_epi16(res, kzero), res)
        } else {
            _mm_and_si128(_mm_cmpgt_epi16(res, kzero), res)
        }
    };

    (res, plo, pmd, phi)
}
