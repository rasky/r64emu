use std::arch::x86_64::*;

trait VMudInst {
    const HIGH: bool;
    const MAC: bool;
}

struct VMudN {}
impl VMudInst for VMudN {
    const HIGH: bool = false;
    const MAC: bool = false;
}

struct VMudH {}
impl VMudInst for VMudH {
    const HIGH: bool = true;
    const MAC: bool = false;
}

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
unsafe fn vmud<I: VMudInst>(
    vs: __m128i,
    vt: __m128i,
    aclo: __m128i,
    acmd: __m128i,
    achi: __m128i,
) -> (__m128i, __m128i, __m128i, __m128i) {
    // Expand to 32-bit: VS:zeroex, VT:signex
    let (vs1, vs2) = _mm_unpack_epi16(vs, I::HIGH);
    let (vt1, vt2) = _mm_unpack_epi16(vt, true);

    let acc1 = _mm_mullo_epi32(vs1, vt1);
    let acc2 = _mm_mullo_epi32(vs2, vt2);

    #[allow(overflowing_literals)]
    let lomask = _mm_set1_epi32(0xFFFF);

    if !I::HIGH {
        let res = _mm_packus_epi32(_mm_and_si128(acc1, lomask), _mm_and_si128(acc2, lomask));
        let acc_lo = res;
        let acc_md = _mm_packus_epi32(_mm_srli_epi32(acc1, 16), _mm_srli_epi32(acc2, 16));
        let acc_hi = _mm_srai_epi16(acc_md, 15);
        (res, acc_lo, acc_md, acc_hi)
    } else {
        let res = _mm_packs_epi32(acc1, acc2);
        let acc_lo = _mm_setzero_si128();
        let acc_md = _mm_packus_epi32(_mm_and_si128(acc1, lomask), _mm_and_si128(acc2, lomask));
        let acc_hi = _mm_packus_epi32(_mm_srli_epi32(acc1, 16), _mm_srli_epi32(acc2, 16));
        (res, acc_lo, acc_md, acc_hi)
    }
}

#[inline]
#[target_feature(enable = "sse4.1")]
pub unsafe fn vmudn(
    vs: __m128i,
    vt: __m128i,
    aclo: __m128i,
    acmd: __m128i,
    achi: __m128i,
) -> (__m128i, __m128i, __m128i, __m128i) {
    vmud::<VMudN>(vs, vt, aclo, acmd, achi)
}

#[inline]
#[target_feature(enable = "sse4.1")]
pub unsafe fn vmudh(
    vs: __m128i,
    vt: __m128i,
    aclo: __m128i,
    acmd: __m128i,
    achi: __m128i,
) -> (__m128i, __m128i, __m128i, __m128i) {
    vmud::<VMudH>(vs, vt, aclo, acmd, achi)
}
