use bit_field::BitField;

// Available Page Masks
const PAGE_MASK_4_KB: u32 = 0b0000_0000_0000_0000_0000_0000;
const PAGE_MASK_16_KB: u32 = 0b0000_0000_0011_0000_0000_0000;
const PAGE_MASK_64_KB: u32 = 0b0000_0000_1111_0000_0000_0000;
const PAGE_MASK_256_KB: u32 = 0b0000_0011_1111_0000_0000_0000;
const PAGE_MASK_1_MB: u32 = 0b0000_1111_1111_0000_0000_0000;
const PAGE_MASK_4_MB: u32 = 0b0011_1111_1111_0000_0000_0000;
const PAGE_MASK_16_MB: u32 = 0b1111_1111_1111_0000_0000_0000;

#[derive(Debug, Clone, Default)]
pub struct TlbEntry {
    /// Page Mask
    page_mask: u32,
    /// Virtual Page Number
    vpn2: u32,
    /// Address Space Identifier
    asid: u8,
    /// Global flag
    global: bool,
    /// EntryLo0
    lo0: u32,
    /// EntryLo1
    lo1: u32,
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
        self.lo0 >> 6
    }

    /// Physical Page Number (1)
    #[inline]
    pub fn pfn1(&self) -> u32 {
        self.lo1 >> 6
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

#[derive(Debug)]
pub struct Tlb(Box<[TlbEntry]>);

impl Default for Tlb {
    fn default() -> Self {
        Tlb(vec![TlbEntry::default(); 32].into_boxed_slice())
    }
}

impl Tlb {
    /// Probes for a matching entry and returns the index if a match is found.
    /// None if no match is found.
    pub fn probe(&self, vaddr: u64, vasid: u8) -> Option<usize> {
        unimplemented!()
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
        entry_lo0: u32,
        entry_lo1: u32,
    ) {
        let entry = &mut self.0[index];

        entry.page_mask = !(page_mask >> 13);
        entry.vpn2 = (entry_hi >> 35 & 0x1800_0000) as u32 | (entry_hi >> 13 & 0x07FF_FFFF) as u32;
        entry.asid = (entry_hi & 0xFF) as u8;
        entry.global = entry_lo0.get_bit(1) && entry_lo1.get_bit(1);

        entry.lo0 = entry_lo0;
        entry.lo1 = entry_lo1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlb() {
        let mut tlb = Tlb::default();

        tlb.write(
            1,
            PAGE_MASK_4_KB,
            0b1000_0000_0000_0000_0000_0000_1111_0000,
            0b0000_0000_0000_0000_1000_0000_0000_0011,
            0b0000_0000_0000_0000_0100_0000_0000_0011,
        );

        let entry = tlb.read(1);
        assert!(entry.global);
        assert!(entry.valid());
        assert_eq!(entry.pfn0(), 0b0000_0000_0000_0000_0010_0000_0000);
        assert_eq!(entry.pfn1(), 0b0000_0000_0000_0000_0001_0000_0000);
        // assert_eq!(entry.hi(), 0b1000_0000_0000_0000_0000_0000_1111_0000);
    }
}
