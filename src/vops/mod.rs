use std::arch::x86_64::*;

/// vops module: implement complex RSP vector operation.
///
/// For some ops there might be multiple implementation, depending on the minimum SSE version.
///
/// NOTE: please do not add tests here. To test ops, add them at the integration level
/// (tests/spvector.rs) so that they can more easily cover all the different implementations
/// (including JIT).

macro_rules! gen_mul_variant {
    ($name:ident, $base:ident, $target:expr, $($arg:expr),*) => {
        #[target_feature(enable = $target)]
        #[inline]
        pub unsafe fn $name(
            vs: __m128i,
            vt: __m128i,
            aclo: __m128i,
            acmd: __m128i,
            achi: __m128i,
        ) -> (__m128i, __m128i, __m128i, __m128i) {
            $base(vs, vt, aclo, acmd, achi, $($arg),*)
        }
    };
}

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

mod vmud;
mod vmud_sse41;
mod vmulf;
mod vmulf_sse41;

#[allow(dead_code)]
pub mod sse2 {
    pub use super::vmud::*;
    pub use super::vmulf::*;
}

#[allow(dead_code)]
pub mod sse41 {
    pub use super::vmud_sse41::*;
    pub use super::vmulf_sse41::*;
}
