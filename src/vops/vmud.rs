use std::arch::x86_64::*;

use super::vmud_sse41::internal_vmudh;
use super::vmud_sse41::internal_vmudn;

gen_mul_variant!(vmudn, internal_vmudn, "sse2", false);
gen_mul_variant!(vmadn, internal_vmudn, "sse2", true);
gen_mul_variant!(vmudh, internal_vmudh, "sse2", false);
gen_mul_variant!(vmadh, internal_vmudh, "sse2", true);
