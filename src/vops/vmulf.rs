use std::arch::x86_64::*;

macro_rules! accum_shl {
    ($lo:ident, $md:ident, $hi:ident, $count:expr) => {{
        const COUNT: i32 = $count;
        const RCOUNT: i32 = 16 - $count;

        let lo2 = _mm_srli_epi16($lo, RCOUNT);
        let reslo = _mm_slli_epi16($lo, COUNT);
        let md2 = _mm_srli_epi16($md, RCOUNT);
        let resmd = _mm_or_si128(_mm_slli_epi16($md, COUNT), lo2);
        let reshi = _mm_or_si128(_mm_slli_epi16($hi, COUNT), md2);
        (reslo, resmd, reshi)
    }};
}

macro_rules! _mm_add_epi16_carry {
    ($a:expr, $b:expr) => {{
        let va = $a;
        let vb = $b;
        #[allow(overflowing_literals)]
        let mask = _mm_set1_epi16(0x8000);
        let res = _mm_add_epi16(va, vb);
        let carry = _mm_srli_epi16(
            _mm_cmpgt_epi16(_mm_xor_si128(va, mask), _mm_xor_si128(res, mask)),
            15,
        );
        (res, carry)
    }};
}

#[allow(unused_macros)]
macro_rules! accum_add {
    ($aclo:expr, $acmd:expr, $achi:expr, $lo:expr, $md:expr, $hi:expr) => {{
        let (aclo, carry1) = _mm_add_epi16_carry!($aclo, $lo);
        let (acmd, carry2) = _mm_add_epi16_carry!($acmd, $md);
        let achi = _mm_add_epi16($achi, $hi);
        let (acmd, carry3) = _mm_add_epi16_carry!(acmd, carry1);
        let achi = _mm_add_epi16(achi, carry2);
        let achi = _mm_add_epi16(achi, carry3);
        (aclo, acmd, achi)
    }};
}

macro_rules! accum_add2 {
    ($aclo:expr, $acmd:expr, $lo:expr, $md:expr, $signed:expr) => {{
        let (aclo, carry1) = _mm_add_epi16_carry!($aclo, $lo);
        let (aclo, acmd) = if $signed {
            // Add higher part, with carry.
            // If it saturates, we need to saturate the lower part as well.
            let acmd = _mm_adds_epi16($acmd, $md);
            let acmdu = _mm_add_epi16($acmd, $md);
            #[allow(overflowing_literals)]
            let aclo = _mm_or_si128(
                _mm_xor_si128(_mm_cmpeq_epi16(acmd, acmdu), _mm_set1_epi16(0xFFFF)),
                aclo,
            );

            let acmd2 = _mm_adds_epi16(acmd, carry1);
            let acmdu2 = _mm_add_epi16(acmd, carry1);
            #[allow(overflowing_literals)]
            let aclo2 = _mm_or_si128(
                _mm_xor_si128(_mm_cmpeq_epi16(acmd2, acmdu2), _mm_set1_epi16(0xFFFF)),
                aclo,
            );

            (aclo2, acmd2)
        } else {
            // FIXME: should probably carry into ACCUM HI?
            // Or maybe not since VMULF/VMACF seem to use a 32-bit accumulator,
            // so maybe it's unsigned 32-bit accumulator for VMULU/MACU too.
            (aclo, _mm_add_epi16(_mm_add_epi16($acmd, $md), carry1))
        };
        (aclo, acmd)
    }};
}

// SSE2 version
#[inline(always)]
unsafe fn internal_vmulfu(
    vs: __m128i,
    vt: __m128i,
    aclo: __m128i,
    acmd: __m128i,
    _achi: __m128i,
    signed: bool,
    mac: bool,
) -> (__m128i, __m128i, __m128i, __m128i) {
    let vzero = _mm_setzero_si128();
    let plo = _mm_mullo_epi16(vs, vt);
    let pmd = _mm_mulhi_epi16(vs, vt);
    let phi = vzero;

    // Left-shift accumulator by 1
    let (plo, pmd, _) = accum_shl!(plo, pmd, phi, 1);

    let (plo, pmd) = if mac {
        accum_add2!(aclo, acmd, plo, pmd, signed)
    } else {
        (plo, pmd)
    };

    // Add 0x8000 to accumulator md/lo (if not VMAC)
    let (plo, pmd) = if !mac {
        #[allow(overflowing_literals)]
        let round = _mm_set1_epi16(0x8000);
        let (plo1, locarry) = _mm_add_epi16_carry!(plo, round);
        let pmd1 = _mm_add_epi16(pmd, locarry);
        (plo1, pmd1)
    } else {
        (plo, pmd)
    };

    // acchi = sign-extend(mid)
    let phi = if mac && !signed {
        vzero
    } else {
        _mm_cmpgt_epi16(vzero, pmd)
    };

    let res = if signed {
        pmd
    } else {
        // Clamp unsigned -> reject negative values
        if !mac {
            _mm_and_si128(_mm_cmpgt_epi16(pmd, vzero), pmd)
        } else {
            _mm_or_si128(_mm_cmplt_epi16(pmd, vzero), pmd)
        }
    };

    (res, plo, pmd, phi)
}

gen_mul_variant!(vmulf, internal_vmulfu, "sse2", true, false);
gen_mul_variant!(vmulu, internal_vmulfu, "sse2", false, false);
gen_mul_variant!(vmacf, internal_vmulfu, "sse2", true, true);
gen_mul_variant!(vmacu, internal_vmulfu, "sse2", false, true);
