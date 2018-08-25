use super::cp0::StatusReg;
use bit_field::BitField;

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

const XKPHYS: [&Segment; 8] = [
    &XKPHYS0, &XKPHYS1, &XKPHYS2, &XKPHYS3, &XKPHYS4, &XKPHYS5, &XKPHYS6, &XKPHYS7,
];

impl Segment {
    #[inline(never)]
    pub fn from_vaddr(vaddr: u64, status: &StatusReg) -> &Segment {
        let ksu = status.ksu();
        let exl = status.exl();
        let erl = status.erl();
        let user_mode = ksu == 0b10 && !exl && !erl;
        let supervisor_mode = ksu == 0b01 && !exl && !erl;
        let kernel_mode = ksu == 0b00 || exl || erl;

        if user_mode {
            // TODO: check for out of bounds
            if status.ux() {
                // 64bit
                &XUSEG
            } else {
                // 32bit
                &USEG
            }
        } else if supervisor_mode {
            if status.sx() {
                // 64bit
                match vaddr.get_bits(62..64) {
                    0b00 => &XSUSEG,
                    0b01 => &XSSEG,
                    0b11 => &CSSEG,
                    _ => {
                        // TODO: proper error handling
                        panic!("invalid address access")
                    }
                }
            } else {
                // 32bit
                if vaddr.get_bit(32) {
                    &SSEG
                } else {
                    &SUSEG
                }
            }
        } else if kernel_mode {
            if status.kx() {
                // 64bit
                match vaddr.get_bits(62..64) {
                    0b00 => &XKUSEG,
                    0b01 => &XKSSEG,
                    0b10 => XKPHYS[vaddr.get_bits(59..62) as usize],
                    0b11 => {
                        if vaddr < CKSEG0.start {
                            &XKSEG
                        } else if vaddr < CKSEG1.start {
                            &CKSEG0
                        } else if vaddr < CKSSEG.start {
                            &CKSEG1
                        } else if vaddr < CKSEG3.start {
                            &CKSSEG
                        } else {
                            &CKSEG3
                        }
                    }
                    _ => unreachable!(),
                }
            } else {
                // 32bit
                if !vaddr.get_bit(31) {
                    &KUSEG
                } else {
                    match vaddr.get_bits(29..32) {
                        0b100 => &KSEG0,
                        0b101 => &KSEG1,
                        0b110 => &KSSEG,
                        0b111 => &KSEG3,
                        _ => panic!("unimplemented address access"),
                    }
                }
            }
        } else {
            panic!("invalid operating mode");
        }
    }
}
