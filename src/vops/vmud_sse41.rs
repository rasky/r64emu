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

#[inline]
#[target_feature(enable = "sse4.1")]
unsafe fn _mm_mullo_epi32_polyfill(v1: __m128i, v2: __m128i, sse41: bool) -> __m128i {
    if sse41 {
        _mm_mullo_epi32(v1, v2)
    } else {
        let tmp1 = _mm_mul_epu32(v1, v2);
        let tmp2 = _mm_mul_epu32(_mm_srli_si128(v1, 4), _mm_srli_si128(v2, 4));
        _mm_unpacklo_epi32(
            _mm_shuffle_epi32(tmp1, 2 << 2),
            _mm_shuffle_epi32(tmp2, 2 << 2),
        )
    }
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse4.1")]
pub(crate) unsafe fn internal_vmud(
    vs: __m128i,
    vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    sse41: bool,
    high: bool,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    // Expand to 32-bit: VS:zeroex, VT:signex
    let (vs1, vs2) = _mm_unpack_epi16(vs, high);
    let (vt1, vt2) = _mm_unpack_epi16(vt, true);

    // ACC = VS*VT (32-bit wide)
    let mut acc1 = _mm_mullo_epi32_polyfill(vs1, vt1, sse41);
    let mut acc2 = _mm_mullo_epi32_polyfill(vs2, vt2, sse41);

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

gen_mul_variant!(vmudn, internal_vmud, "sse4.1", true, false, false);
gen_mul_variant!(vmudh, internal_vmud, "sse4.1", true, true, false);
gen_mul_variant!(vmadn, internal_vmud, "sse4.1", true, false, true);
gen_mul_variant!(vmadh, internal_vmud, "sse4.1", true, true, true);
