mod sp;
pub use self::sp::*;

/// NOTE: please do not add tests here. To test ops, add them at the integration level
/// (tests/spvector.rs) so that they can more easily cover all the different implementations
/// (including JIT).
mod accumulator;
mod cop0;
mod cop2;
mod vmul;
