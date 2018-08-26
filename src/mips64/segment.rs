use super::cp0::StatusReg;
use bit_field::BitField;
use phf;

macro_rules! seg {
    ($name:ident, $start:expr, $length:expr, $cached:expr, $mapped:expr) => {
        const $name: Segment = Segment {
            start: $start,
            length: $length,
            cached: $cached,
            mapped: $mapped,
        };
    };
}

pub struct Segment {
    pub start: u64,
    pub length: u64,

    /// Is the memory cached?
    pub cached: bool,
    /// Is this segement TLB mapped?
    pub mapped: bool,
}

// User Mode

seg!(
    USEG,
    0x0000_0000_0000_0000,
    0x0000_0000_8000_0000,
    true,
    true
);
seg!(
    XUSEG,
    0x0000_0000_0000_0000,
    0x0000_0100_0000_0000,
    true,
    true
);

// Supervisor Mode
seg!(
    SUSEG,
    0x0000_0000_0000_0000,
    0x0000_0000_8000_0000,
    true,
    true
);
seg!(
    SSEG,
    0xFFFF_FFFF_C000_0000,
    0x0000_0000_2000_0000,
    true,
    true
);

seg!(
    XSUSEG,
    0x0000_0000_0000_0000,
    0x0000_0100_0000_0000,
    true,
    true
);
seg!(
    XSSEG,
    0x4000_0000_0000_0000,
    0x0000_0100_0000_0000,
    true,
    true
);
seg!(
    CSSEG,
    0xFFFF_FFFF_C000_0000,
    0x0000_0000_1FFF_FFFF,
    true,
    true
);

static SEGMENTS: phf::OrderedMap<[u8; 4], &Segment> = phf_ordered_map! {
    // -- User
    // 32bit
    [0b10, 0, 0, 0] => &USEG,
    // 64bit
    [0b10, 1, 0, 0] => &XUSEG,

    // -- Supervisor
    // 32bit
    [0b01, 0, 0, 0] => &SUSEG,
    [0b01, 0, 1, 0] => &SSEG,
    // 64bit
    [0b01, 1, 0b00, 0] => &XSUSEG,
    [0b01, 1, 0b01, 0] => &XSSEG,
    [0b01, 1, 0b11, 0] => &CSSEG,

    // -- Kernel
    // 32bit
    [0b00, 0, 0b000, 0] => &KUSEG,
    [0b00, 0, 0b001, 0] => &KUSEG,
    [0b00, 0, 0b010, 0] => &KUSEG,
    [0b00, 0, 0b011, 0] => &KUSEG,
    [0b00, 0, 0b100, 0] => &KSEG0,
    [0b00, 0, 0b101, 0] => &KSEG1,
    [0b00, 0, 0b110, 0] => &KSSEG,
    [0b00, 0, 0b111, 0] => &KSEG3,

    // 64bit
    [0b00, 1, 0b00, 0] => &XKUSEG,
    [0b00, 1, 0b01, 0] => &XKSSEG,
    [0b00, 1, 0b10, 0] => &XKPHYS0,
    [0b00, 1, 0b10, 1] => &XKPHYS1,
    [0b00, 1, 0b10, 2] => &XKPHYS2,
    [0b00, 1, 0b10, 3] => &XKPHYS3,
    [0b00, 1, 0b10, 4] => &XKPHYS4,
    [0b00, 1, 0b10, 5] => &XKPHYS5,
    [0b00, 1, 0b10, 6] => &XKPHYS6,
    [0b00, 1, 0b10, 7] => &XKPHYS7,
    [0b00, 1, 0b11, 4] => &XKSEG,
    [0b00, 1, 0b11, 0] => &CKSEG0,
    [0b00, 1, 0b11, 1] => &CKSEG1,
    [0b00, 1, 0b11, 2] => &CKSSEG,
    [0b00, 1, 0b11, 3] => &CKSEG3,
};

// Placeholder, in arrays
seg!(NULL, 0, 0, false, false);

// Kernel Mode
seg!(
    KUSEG,
    0x0000_0000_0000_0000,
    0x0000_0000_8000_0000,
    true,
    true
);
seg!(
    KSEG0,
    0xFFFF_FFFF_8000_0000,
    0x0000_0000_2000_0000,
    true,
    false
);
seg!(
    KSEG1,
    0xFFFF_FFFF_A000_0000,
    0x0000_0000_2000_0000,
    true,
    false
);
seg!(
    KSSEG,
    0xFFFF_FFFF_C000_0000,
    0x0000_0000_2000_0000,
    true,
    true
);
seg!(
    KSEG3,
    0xFFFF_FFFF_E000_0000,
    0x0000_0000_2000_0000,
    true,
    true
);

seg!(
    XKUSEG,
    0x0000_0000_0000_0000,
    0x0000_0100_0000_0000,
    true,
    true
);
seg!(
    XKSSEG,
    0x4000_0000_0000_0000,
    0x0000_0100_0000_0000,
    true,
    true
);
seg!(
    XKPHYS0,
    0x8000_0000_0000_0000,
    0x0000_0001_0000_0000,
    true,
    false
);
seg!(
    XKPHYS1,
    0x8800_0000_0000_0000,
    0x0000_0001_0000_0000,
    true,
    false
);
seg!(
    XKPHYS2,
    0x9000_0000_0000_0000,
    0x0000_0001_0000_0000,
    false,
    false
);
seg!(
    XKPHYS3,
    0x9800_0000_0000_0000,
    0x0000_0001_0000_0000,
    true,
    false
);
seg!(
    XKPHYS4,
    0xA000_0000_0000_0000,
    0x0000_0001_0000_0000,
    true,
    false
);
seg!(
    XKPHYS5,
    0xA800_0000_0000_0000,
    0x0000_0001_0000_0000,
    true,
    false
);
seg!(
    XKPHYS6,
    0xB000_0000_0000_0000,
    0x0000_0001_0000_0000,
    true,
    false
);
seg!(
    XKPHYS7,
    0xB800_0000_0000_0000,
    0x0000_0001_0000_0000,
    true,
    false
);
seg!(
    XKSEG,
    0xC000_0000_0000_0000,
    0x0000_0100_0000_0000,
    true,
    true
);
seg!(
    CKSEG0,
    0xFFFF_FFFF_8000_0000,
    0x0000_0000_2000_0000,
    true,
    false
);
seg!(
    CKSEG1,
    0xFFFF_FFFF_A000_0000,
    0x0000_0000_2000_0000,
    true,
    false
);
seg!(
    CKSSEG,
    0xFFFF_FFFF_C000_0000,
    0x0000_0000_2000_0000,
    true, // cache depends on C bit in the TLB entry
    true
);
seg!(
    CKSEG3,
    0xFFFF_FFFF_E000_0000,
    0x0000_0000_2000_0000,
    true, // cache depends on C bit in the TLB entry
    true
);

impl Segment {
    #[inline(never)]
    pub fn from_vaddr(vaddr: u64, status: &StatusReg) -> &Segment {
        let ksu = status.ksu() as u8;
        let exl = status.exl();
        let erl = status.erl();
        // let user_mode = ksu == 0b10 && !exl && !erl;
        let supervisor_mode = ksu == 0b01 && !exl && !erl;
        let kernel_mode = ksu == 0b00 || exl || erl;
        let use64 = status.ux() | status.sx() | status.kx();

        // TODO: make the below stuff simpler & faster
        let info = if kernel_mode {
            if use64 {
                let b1 = vaddr.get_bits(62..64) as u8;
                let b2 = if b1 == 0b11 {
                    if vaddr < CKSEG0.start {
                        4u8
                    } else if vaddr < CKSEG1.start {
                        0u8
                    } else if vaddr < CKSSEG.start {
                        1u8
                    } else if vaddr < CKSEG3.start {
                        2u8
                    } else {
                        3u8
                    }
                } else {
                    vaddr.get_bits(59..62) as u8
                };

                (b1, b2)
            } else {
                (vaddr.get_bits(29..32) as u8, 0u8)
            }
        } else if supervisor_mode {
            if use64 {
                (vaddr.get_bits(62..64) as u8, 0u8)
            } else {
                (vaddr.get_bit(32) as u8, 0u8)
            }
        } else {
            (0u8, 0u8)
        };

        SEGMENTS
            .get(&[ksu, use64 as u8, info.0, info.1])
            .expect("invalid address access")
    }
}
