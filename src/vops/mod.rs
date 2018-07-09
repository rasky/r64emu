mod vmulf;
mod vmulf_sse41;

pub mod sse2 {
    pub use super::vmulf::vmulf;
}

pub mod sse41 {
    pub use super::vmulf_sse41::vmulf;
}
