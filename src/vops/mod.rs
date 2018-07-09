/// vops module: implement complex RSP vector operation.
///
/// For some ops there might be multiple implementation, depending on the minimum SSE version.
///
/// NOTE: please do not add tests here. To test ops, add them at the integration level
/// (tests/spvector.rs) so that they can more easily cover all the different implementations
/// (including JIT).

macro_rules! gen_mul_variant {
    ($name:ident, $base:ident, $arg1:expr) => {
        #[inline]
        pub unsafe fn $name(
            vs: __m128i,
            vt: __m128i,
            aclo: __m128i,
            acmd: __m128i,
            achi: __m128i,
        ) -> (__m128i, __m128i, __m128i, __m128i) {
            $base(vs, vt, aclo, acmd, achi, $arg1)
        }
    };

    ($name:ident, $base:ident, $arg1:expr, $arg2:expr) => {
        #[inline]
        pub unsafe fn $name(
            vs: __m128i,
            vt: __m128i,
            aclo: __m128i,
            acmd: __m128i,
            achi: __m128i,
        ) -> (__m128i, __m128i, __m128i, __m128i) {
            $base(vs, vt, aclo, acmd, achi, $arg1, $arg2)
        }
    };
}

mod vmud_sse41;
mod vmulf;
mod vmulf_sse41;

#[allow(dead_code)]
pub mod sse2 {
    pub use super::vmulf::*;
}

#[allow(dead_code)]
pub mod sse41 {
    pub use super::vmud_sse41::*;
    pub use super::vmulf_sse41::*;
}
