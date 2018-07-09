/// vops module: implement complex RSP vector operation.
///
/// For some ops there might be multiple implementation, depending on the minimum SSE version.
///
/// NOTE: please do not add tests here. To test ops, add them at the integration level
/// (tests/spvector.rs) so that they can more easily cover all the different implementations
/// (including JIT).
mod vmud_sse41;
mod vmulf;
mod vmulf_sse41;

#[allow(dead_code)]
pub mod sse2 {
    pub use super::vmulf::vmulf;
}

#[allow(dead_code)]
pub mod sse41 {
    pub use super::vmud_sse41::*;
    pub use super::vmulf_sse41::*;
}
