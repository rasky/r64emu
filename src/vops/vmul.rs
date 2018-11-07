use super::{acc_add, acc_clamp_signed, acc_clamp_unsigned2, acc_clamp_unsigned3};
use std::arch::x86_64::*;

// SSE 4.1 version
#[inline]
#[target_feature(enable = "sse2")]
unsafe fn internal_vmulfu(
    vs: __m128i,
    vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    signed: bool,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
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
    let mut acc_hi = _mm_packs_epi32(_mm_srai_epi32(acc1, 31), _mm_srai_epi32(acc2, 31));

    // Multiply by two
    acc1 = _mm_slli_epi32(acc1, 1);
    acc2 = _mm_slli_epi32(acc2, 1);

    // Repack partial result into 16-bit registers
    let klomask = _mm_set1_epi32(0xFFFF);
    let mut acc_lo = _mm_packus_epi32(_mm_and_si128(acc1, klomask), _mm_and_si128(acc2, klomask));
    let mut acc_md = _mm_packus_epi32(_mm_srli_epi32(acc1, 16), _mm_srli_epi32(acc2, 16));

    if mac {
        let (new_acc_lo, new_acc_md, new_acc_hi) =
            acc_add(old_acc_lo, old_acc_md, old_acc_hi, acc_lo, acc_md, acc_hi);

        acc_lo = new_acc_lo;
        acc_md = new_acc_md;
        acc_hi = new_acc_hi;
    }

    let res = if signed {
        acc_clamp_signed(acc_md, acc_hi)
    } else {
        acc_clamp_unsigned2(acc_md, acc_hi)
    };

    (res, acc_lo, acc_md, acc_hi)
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse2")]
pub(crate) unsafe fn internal_vmudnm(
    mut vs: __m128i,
    mut vt: __m128i,
    old_acc_lo: __m128i,
    old_acc_md: __m128i,
    old_acc_hi: __m128i,
    mac: bool,
    mid: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    let (vs, vt) = if mid { (vt, vs) } else { (vs, vt) };

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

    let mut res = if mid { acc_md } else { acc_lo };

    // The clamping is always performed, but we can avoid doing it
    // in the non-mac case as acc_hi is always a sign-extension of acc_md,
    // so there's nothing to do.
    if mac {
        if mid {
            res = acc_clamp_unsigned2(res, acc_hi);
        } else {
            res = acc_clamp_unsigned3(res, acc_md, acc_hi);
        }
    }

    (res, acc_lo, acc_md, acc_hi)
}

// SSE 4.1 version
#[inline] // FIXME: for some reason, Rust doesn't allow inline(always) here
#[target_feature(enable = "sse2")]
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
#[target_feature(enable = "sse2")]
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
    acc_lo = _mm_add_epi16(
        acc_lo,
        _mm_and_si128(_mm_and_si128(vt, toggle), _mm_srai_epi16(vs, 15)),
    );
    acc_lo = _mm_add_epi16(
        acc_lo,
        _mm_and_si128(_mm_and_si128(vs, toggle), _mm_srai_epi16(vt, 15)),
    );

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
        res = acc_clamp_unsigned3(res, acc_md, acc_hi);
    }

    (res, acc_lo, acc_md, acc_hi)
}

gen_mul_variant!(vmudn, internal_vmudnm, "sse2", false, false);
gen_mul_variant!(vmadn, internal_vmudnm, "sse2", true, false);
gen_mul_variant!(vmudm, internal_vmudnm, "sse2", false, true);
gen_mul_variant!(vmadm, internal_vmudnm, "sse2", true, true);

gen_mul_variant!(vmudh, internal_vmudh, "sse2", false);
gen_mul_variant!(vmadh, internal_vmudh, "sse2", true);

gen_mul_variant!(vmudl, internal_vmudl, "sse2", false);
gen_mul_variant!(vmadl, internal_vmudl, "sse2", true);

gen_mul_variant!(vmulf, internal_vmulfu, "sse2", true, false);
gen_mul_variant!(vmulu, internal_vmulfu, "sse2", false, false);
gen_mul_variant!(vmacf, internal_vmulfu, "sse2", true, true);
gen_mul_variant!(vmacu, internal_vmulfu, "sse2", false, true);
