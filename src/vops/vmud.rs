use std::arch::x86_64::*;

use super::vmud_sse41::internal_vmudh;
use super::vmud_sse41::internal_vmudnm;

gen_mul_variant!(vmudn, internal_vmudnm, "sse2", false, false);
gen_mul_variant!(vmadn, internal_vmudnm, "sse2", true, false);
gen_mul_variant!(vmudh, internal_vmudh, "sse2", false);
gen_mul_variant!(vmadh, internal_vmudh, "sse2", true);
