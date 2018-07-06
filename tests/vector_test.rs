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
        spv.set_reg(0, 0x0400_7000_7000_9FFF_0000_3333_FFFF_0001);
        spv.set_reg(1, 0x0300_2000_F000_9FFF_0000_4444_0002_0001);
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
            (0, 0x0400_7000_7000_9FFF_0000_3333_FFFF_0001),
            (1, 0x0300_2000_F000_9FFF_0000_4444_0002_0001),
            (SpVector::REG_VCC, 0x00_4F),
        ],
        vec![I::Vu(O::VADD, 0, 1, 0, 2)],
        vec![
            (2, 0x0700_7FFF_6000_8000_0001_7778_0002_0003),
            (SpVector::REG_VCC, 0),
        ],
    )
}
