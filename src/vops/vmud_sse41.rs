use std::arch::x86_64::*;

// Convert a 16-bit packed register into 2 32-bit packed registers with the same values
// usign either sign extension (signed integers) o zero extension (unsigned integers).
#[inline(always)]
unsafe fn _mm_unpack_epi16(v: __m128i, sign_extend: bool) -> (__m128i, __m128i) {
    if sign_extend {
        (
            _mm_srai_epi32(_mm_unpacklo_epi16(v, v), 16),
            _mm_srai_epi32(_mm_unpackhi_epi16(v, v), 16),
        )
    } else {
        (
            _mm_srli_epi32(_mm_unpacklo_epi16(v, v), 16),
            _mm_srli_epi32(_mm_unpackhi_epi16(v, v), 16),
        )
    }
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse4.1")]
unsafe fn internal_vmud(
    vs: __m128i,
    vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    high: bool,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    // Expand to 32-bit: VS:zeroex, VT:signex
    let (vs1, vs2) = _mm_unpack_epi16(vs, high);
    let (vt1, vt2) = _mm_unpack_epi16(vt, true);

    let mut acc1 = _mm_mullo_epi32(vs1, vt1);
    let mut acc2 = _mm_mullo_epi32(vs2, vt2);

    #[allow(overflowing_literals)]
    let lomask = _mm_set1_epi32(0xFFFF);

    if !high {
        if mac {
            acc1 = _mm_add_epi32(acc1, _mm_unpacklo_epi16(old_acc_lo, old_acc_md));
            acc2 = _mm_add_epi32(acc2, _mm_unpackhi_epi16(old_acc_lo, old_acc_md));
        }
        let res = _mm_packus_epi32(_mm_and_si128(acc1, lomask), _mm_and_si128(acc2, lomask));
        let acc_lo = res;
        let acc_md = _mm_packus_epi32(_mm_srli_epi32(acc1, 16), _mm_srli_epi32(acc2, 16));
        let acc_hi = _mm_srai_epi16(acc_md, 15);
        (res, acc_lo, acc_md, acc_hi)
    } else {
        if mac {
            acc1 = _mm_add_epi32(acc1, _mm_unpacklo_epi16(old_acc_md, old_acc_hi));
            acc2 = _mm_add_epi32(acc2, _mm_unpackhi_epi16(old_acc_md, old_acc_hi));
        }
        let res = _mm_packs_epi32(acc1, acc2);
        let acc_lo = if mac { old_acc_lo } else { _mm_setzero_si128() };
        let acc_md = _mm_packus_epi32(_mm_and_si128(acc1, lomask), _mm_and_si128(acc2, lomask));
        let acc_hi = _mm_packus_epi32(_mm_srli_epi32(acc1, 16), _mm_srli_epi32(acc2, 16));
        (res, acc_lo, acc_md, acc_hi)
    }
}

gen_mul_variant!(vmudn, internal_vmud, "sse4.1", false, false);
gen_mul_variant!(vmudh, internal_vmud, "sse4.1", true, false);
gen_mul_variant!(vmadn, internal_vmud, "sse4.1", false, true);
gen_mul_variant!(vmadh, internal_vmud, "sse4.1", true, true);
