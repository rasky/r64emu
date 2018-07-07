#[macro_use]
extern crate slog;

extern crate byteorder;
extern crate emu;
extern crate r64emu;
extern crate slog_term;

use byteorder::BigEndian;
use emu::bus::be::{Bus, DevPtr};
use emu::sync::Subsystem;
use r64emu::sp::{Sp, SpCop0};
use r64emu::spvector::SpVector;
use slog::Drain;
use std::cell::RefCell;
use std::rc::Rc;

fn logger() -> slog::Logger {
    let decorator = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    slog::Logger::root(drain, o!())
}

fn make_sp() -> (DevPtr<Sp>, Rc<RefCell<Box<Bus>>>) {
    let logger = logger();
    let main_bus = Rc::new(RefCell::new(Bus::new(logger.new(o!()))));
    let sp = Sp::new(logger.new(o!()), main_bus.clone()).unwrap();
    {
        let spb = sp.borrow();
        let mut cpu = spb.core_cpu.borrow_mut();
        cpu.set_cop0(SpCop0::new(&sp));
        cpu.set_cop2(SpVector::new(&sp, logger.new(o!())));
    }
    {
        let mut bus = main_bus.borrow_mut();
        bus.map_device(0x0400_0000, &sp, 0);
        bus.map_device(0x0404_0000, &sp, 1);
        bus.map_device(0x0408_0000, &sp, 2);
    }
    (sp, main_bus)
}

// SP opcodes
enum O {
    VADD = 0b010000,
    VADDC = 0b010100,
    VSAR = 0b011101,
    BREAK = 0b001101,
}

// SP Instruction Types
enum I {
    Vu(O, u32, u32, u32, u32), // vs,vt,e,vd
    SuSpecial(O),
}

fn asm(inst: I) -> u32 {
    match inst {
        I::SuSpecial(op) => 0u32 << 26 | op as u32,
        I::Vu(op, vs, vt, e, vd) => {
            if e > 0xF || vt > 0x1F || vs > 0x1F || vd > 0x1F {
                panic!("invalid TypeVu")
            }
            (0b010010u32 << 26)
                | (1u32 << 25)
                | (e << 21)
                | (vt << 16)
                | (vs << 11)
                | (vd << 6)
                | (op as u32)
        }
    }
}

fn test_vector(
    testname: &str,
    sp: &DevPtr<Sp>,
    main_bus: &Rc<RefCell<Box<Bus>>>,
    inregs: Vec<(usize, u128)>,
    insn: Vec<I>,
    outregs: Vec<(usize, u128)>,
) {
    let cpu = sp.borrow().core_cpu.clone();

    {
        let mut cpu = cpu.borrow_mut();
        let spv = cpu.cop2().unwrap();
        for (idx, val) in inregs {
            spv.set_reg(idx, val)
        }
    }

    {
        let spb = sp.borrow();

        let mut addr = 0u32;
        for i in insn {
            spb.imem.write::<BigEndian, u32>(addr, asm(i));
            addr += 4;
        }
        spb.imem
            .write::<BigEndian, u32>(addr, asm(I::SuSpecial(O::BREAK)));
    }

    main_bus.borrow().write::<u32>(0x0404_0010, 1 << 0); // REG_STATUS = release halt
    cpu.borrow_mut().run(10000);

    {
        let mut cpu = cpu.borrow_mut();
        let spv = cpu.cop2().unwrap();

        for (idx, exp) in outregs {
            let found = spv.reg(idx);
            if found != exp {
                assert!(
                    false,
                    "{}: invalid outreg {}:\nFound: {:x}\nExp:   {:x}",
                    testname, idx, found, exp
                );
            }
        }
    }
}

#[test]
fn vadd() {
    let (sp, main_bus) = make_sp();

    test_vector(
        "vadd1",
        &sp,
        &main_bus,
        vec![
            // carry:
            //       0    1    0    0    1    1    1    1  (= 0xF2 right-to-left)
            (0, 0x0400_7000_7000_9FFF_0000_3333_FFFF_0001),
            (1, 0x0300_2000_F000_9FFF_0000_4444_0002_0001),
            (SpVector::REG_VCO, 0xAA_F2), // F2 is VCO-carry (used); AA is VCO-NE (ignored)
        ],
        vec![I::Vu(O::VADD, 0, 1, 0, 2)],
        vec![
            (2, 0x0700_7FFF_6000_8000_0001_7778_0002_0003),
            (SpVector::REG_VCO, 0), // ne+carry should be zero after VADD
            (
                SpVector::REG_ACCUM_LO,
                0x0700_9001_6000_3FFE_0001_7778_0002_0003, // non-saturated add
            ),
        ],
    )
}

#[test]
fn vaddc() {
    let (sp, main_bus) = make_sp();

    test_vector(
        "vaddc",
        &sp,
        &main_bus,
        vec![
            (0, 0x0400_7000_7000_9FFF_0000_3333_FFFF_0001),
            (1, 0x0300_2000_F000_9FFF_0000_4444_0002_0001),
            (SpVector::REG_VCO, 0xFF_FF), // carry/ne should be ignored
        ],
        vec![I::Vu(O::VADDC, 0, 1, 0, 2)],
        vec![
            (2, 0x0700_9000_6000_3FFE_0000_7777_0001_0002),
            // expected carry:
            //       0    0    1    1    0    0    1    0  (= 0x4C right-to-left)
            (SpVector::REG_VCO, 0x00_4C),
            (
                SpVector::REG_ACCUM_LO,
                0x0700_9000_6000_3FFE_0000_7777_0001_0002,
            ),
        ],
    )
}

#[test]
fn vsar() {
    let (sp, main_bus) = make_sp();

    test_vector(
        "vsar1",
        &sp,
        &main_bus,
        vec![
            (0, 0x1212_3434_5656_7878_9A9A_BCBC_DEDE_F0F0),
            (1, 0x0110_2332_4554_6776_8998_ABBA_CDDC_EFFE),
            (2, 0xFDEC_BA98_7654_3210_0123_4567_89AB_CDEF),
            (
                SpVector::REG_ACCUM_LO,
                0xAAAA_BBBB_CCCC_DDDD_EEEE_FFFF_0000_1111,
            ),
            (
                SpVector::REG_ACCUM_MD,
                0x2222_3333_4444_5555_6666_7777_8888_9999,
            ),
            (
                SpVector::REG_ACCUM_HI,
                0x0AA0_0BB0_FCCF_0DD0_0EE0_0FF0_F00F_0110,
            ),
        ],
        vec![
            I::Vu(O::VSAR, 0, 0, 10, 20),
            I::Vu(O::VSAR, 1, 0, 9, 21),
            I::Vu(O::VSAR, 2, 0, 8, 22),
        ],
        vec![
            (20, 0xAAAA_BBBB_CCCC_DDDD_EEEE_FFFF_0000_1111),
            (21, 0x2222_3333_4444_5555_6666_7777_8888_9999),
            (22, 0x0AA0_0BB0_FCCF_0DD0_0EE0_0FF0_F00F_0110),
            (
                SpVector::REG_ACCUM_LO,
                0x1212_3434_5656_7878_9A9A_BCBC_DEDE_F0F0,
            ),
            (
                SpVector::REG_ACCUM_MD,
                0x0110_2332_4554_6776_8998_ABBA_CDDC_EFFE,
            ),
            (
                SpVector::REG_ACCUM_HI,
                0xFDEC_BA98_7654_3210_0123_4567_89AB_CDEF,
            ),
        ],
    )
}
