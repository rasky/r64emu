use bit_field::BitField;
use emu::int::Numerics;
use std::fmt;

#[derive(Copy, Clone, Default)]
pub struct TlbEntry {
    /// Page Mask
    pub page_mask: u32,
    /// Virtual Page Number
    pub vpn2: u32,
    /// Address Space Identifier
    pub asid: u8,
    /// Global flag
    pub global: bool,
    /// EntryLo0
    pub lo0: u64,
    /// EntryLo1
    pub lo1: u64,
}

impl TlbEntry {
    /// Returns the the entry_hi for this entry.
    #[inline]
    pub fn hi(&self) -> u64 {
        ((self.vpn2 & 0x1800_0000) as u64) << 35
            | ((self.vpn2 & 0x07FF_FFFF) as u64) << 13
            | (self.global as u64) << 12
            | self.asid as u64
    }

    /// Physical Page Number (0)
    #[inline]
    pub fn pfn0(&self) -> u32 {
        ((self.lo0 << 6) & 0xFFFF_F000) as u32
    }

    /// Physical Page Number (1)
    #[inline]
    pub fn pfn1(&self) -> u32 {
        ((self.lo1 << 6) & 0xFFFF_F000) as u32
    }

    #[inline]
    pub fn dirty0(&self) -> bool {
        self.lo0.get_bit(2)
    }

    #[inline]
    pub fn dirty1(&self) -> bool {
        self.lo1.get_bit(2)
    }

    #[inline]
    pub fn valid(&self) -> bool {
        self.valid0() || self.valid1()
    }

    #[inline]
    pub fn valid0(&self) -> bool {
        self.lo0.get_bit(1)
    }

    #[inline]
    pub fn valid1(&self) -> bool {
        self.lo1.get_bit(1)
    }
}

impl fmt::Debug for TlbEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.debug_struct("TlbEntry")
            .field("page_mask", &self.page_mask.hex())
            .field("vpn2", &self.vpn2.hex())
            .field("asid", &self.asid.hex())
            .field("global", &self.global)
            .field("lo0", &self.lo0.hex())
            .field("lo1", &self.lo1.hex())
            .finish()
    }
}

// Memory mapping unit of a MIPS processor
#[derive(Debug)]
pub struct Mmu([TlbEntry; 32]);

impl Default for Mmu {
    fn default() -> Self {
        Mmu([TlbEntry::default(); 32])
    }
}

impl Mmu {
    /// Probes for a matching entry and returns the index if a match is found.
    /// None if no match is found.
    pub fn probe(&self, vaddr: u64, vasid: u8) -> Option<usize> {
        let vpn2 = calc_vpn2(vaddr);

        for (i, entry) in self.0.iter().enumerate() {
            let asid_match = entry.global || entry.asid == vasid;
            let vpn_match = entry.vpn2 == vpn2 & !(entry.page_mask >> 13);

            if asid_match && vpn_match {
                return Some(i);
            }
        }

        None
    }

    /// Reads a specific TLB index.
    pub fn read(&self, index: usize) -> &TlbEntry {
        &self.0[index]
    }

    /// Writes an entry at the specified index to the TLB.
    pub fn write(
        &mut self,
        index: usize,
        page_mask: u32,
        entry_hi: u64,
        entry_lo0: u64,
        entry_lo1: u64,
    ) {
        let entry = &mut self.0[index];

        entry.page_mask = page_mask;
        entry.vpn2 = calc_vpn2(entry_hi);

        entry.asid = entry_hi as u8;
        entry.global = entry_lo0.get_bit(0) && entry_lo1.get_bit(0);

        entry.lo0 = entry_lo0;
        entry.lo1 = entry_lo1;
    }
}

pub fn calc_vpn2(addr: u64) -> u32 {
    (addr >> 35 & 0x1800_0000) as u32 | (addr >> 13 & 0x07FF_FFFF) as u32
}

#[cfg(test)]
mod tests {
    extern crate test;
    use self::test::Bencher;
    use super::*;

    // Available Page Masks
    const PAGE_MASK_4_KB: u32 = 0b0000_0000_0000_0000_0000_0000;
    const PAGE_MASK_16_KB: u32 = 0b0000_0000_0011_0000_0000_0000;
    const PAGE_MASK_64_KB: u32 = 0b0000_0000_1111_0000_0000_0000;
    const PAGE_MASK_256_KB: u32 = 0b0000_0011_1111_0000_0000_0000;
    const PAGE_MASK_1_MB: u32 = 0b0000_1111_1111_0000_0000_0000;
    const PAGE_MASK_4_MB: u32 = 0b0011_1111_1111_0000_0000_0000;
    const PAGE_MASK_16_MB: u32 = 0b1111_1111_1111_0000_0000_0000;

    #[test]
    fn test_mmu() {
        let mut mmu = Mmu::default();

        mmu.write(
            1,
            PAGE_MASK_4_KB,
            0b1110_0000_0000_0000_0000_0000_1111_0000_1001_1001,
            0b0000_0000_0000_0000_1000_0000_0000_0010, // valid, not global
            0b0000_0000_0000_0000_0100_0000_0000_0010, // valid, not global
        );
        mmu.write(
            2,
            PAGE_MASK_4_KB,
            0b1111_0000_0000_0000_0000_0000_1111_0000_1001_1001,
            0b0000_0000_0000_0000_1000_0000_0000_0011, // global and valid
            0b0000_0000_0000_0000_0100_0000_0000_0011, // global and valid
        );

        let entry = mmu.read(1);
        assert!(!entry.global);
        assert!(entry.valid());
        assert_eq!(entry.asid, 0b1001_1001);
        assert_eq!(entry.pfn0(), 0b0000_0010_0000_0000_0000_0000_0000);
        assert_eq!(entry.pfn1(), 0b0000_0001_0000_0000_0000_0000_0000);
        assert_eq!(entry.vpn2, 0b0111_0000_0000_0000_0000_0000_0111);

        assert_eq!(
            entry.hi(),
            0b1110_0000_0000_0000_0000_0000_1110_0000_1001_1001,
        );

        assert_eq!(
            mmu.probe(
                0b0000_1110_0000_0000_0000_0000_0000_1111_0000_0000_0001,
                0b1001_1001,
            ),
            Some(1)
        );
        // non matching asid
        assert_eq!(
            mmu.probe(
                0b0000_1110_0000_0000_0000_0000_0000_1111_0000_0000_0001,
                0b1001_1000,
            ),
            None
        );
        // non matching vpn2
        assert_eq!(
            mmu.probe(
                0b0000_1110_0000_0000_0000_0000_0000_0111_0000_0000_0001,
                0b1001_1001,
            ),
            None
        );

        // non matching asid - with global
        assert_eq!(
            mmu.probe(
                0b0000_1111_0000_0000_0000_0000_0000_1111_0000_0000_0001,
                0b1001_1000,
            ),
            Some(2)
        );
    }

    #[bench]
    fn bench_tlb_probe_match(b: &mut Bencher) {
        let mut mmu = Mmu::default();

        mmu.write(
            13,
            PAGE_MASK_4_KB,
            0b1110_0000_0000_0000_0000_0000_1111_0000_1001_1001,
            0b0000_0000_0000_0000_1000_0000_0000_0010, // valid, not global
            0b0000_0000_0000_0000_0100_0000_0000_0010, // valid, not global
        );

        // Current baseline 13 ns/iter
        b.iter(|| {
            mmu.probe(
                0b0000_1110_0000_0000_0000_0000_0000_1111_0000_0000_0001,
                0b1001_1001,
            )
        });
    }
}
