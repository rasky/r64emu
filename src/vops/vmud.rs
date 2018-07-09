use std::arch::x86_64::*;

use super::vmud_sse41::internal_vmud;

// Reuse SSE 4.1 version with a polyfiller for a single instruction (_mm_mullo_epi32)
gen_mul_variant!(vmudn, internal_vmud, "sse2", false, false, false);
gen_mul_variant!(vmudh, internal_vmud, "sse2", false, true, false);
gen_mul_variant!(vmadn, internal_vmud, "sse2", false, false, true);
gen_mul_variant!(vmadh, internal_vmud, "sse2", false, true, true);
