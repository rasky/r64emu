use super::n64::R4300;
use emu::bus::be::Reg32;
use emu::bus::DeviceGetter;
use mips64::Cop0;

use bit_field::BitField;
use bitflags::bitflags;
use slog;

use std::cell::RefCell;
use std::rc::Rc;

bitflags! {
    pub struct IrqMask: u32 {
        const SP =            0b00000001;
        const SI =            0b00000010;
        const AI =            0b00000100;
        const VI =            0b00001000;
        const PI =            0b00010000;
        const DP =            0b00100000;
    }
}

#[derive(DeviceBE)]
pub struct Mi {
    #[reg(offset = 0x08, readonly)]
    irq_ack: Reg32,

    #[reg(offset = 0x0C, wcb)]
    irq_mask: Reg32,

    logger: slog::Logger,
    cop0: Rc<RefCell<Box<mips64::Cp0>>>,
}

impl Mi {
    pub fn new(logger: slog::Logger) -> Mi {
        Mi {
            irq_ack: Reg32::default(),
            irq_mask: Reg32::default(),
            logger,
            cop0: R4300::get().cop0_clone(),
        }
    }

    pub fn set_irq_line(&mut self, mask: IrqMask, status: bool) {
        let old = self.irq_ack.get();
        let new = if status {
            old | mask.bits()
        } else {
            old & !mask.bits()
        };
        self.irq_ack.set(new);
        if old != new {
            info!(self.logger, "changed IRQ ack"; "irq" => ?IrqMask::from_bits(new));

            // FIXME: this reentrancy will eventually panic (CPU -> BUS -> Device -> CPU).
            // Find out a way to fix this.
            if new != 0 {
                self.cop0.borrow_mut().set_hwint_line(0, true);
            } else {
                self.cop0.borrow_mut().set_hwint_line(0, false);
            }
        }
    }

    fn cb_write_irq_mask(&mut self, old: u32, new: u32) {
        let mut mask = old;
        for i in 0..12 {
            if new.get_bit(i) {
                mask.set_bit(i / 2, i % 2 != 0);
            }
        }
        self.irq_mask.set(mask);
        info!(self.logger, "changed IRQ mask"; "irq" => ?IrqMask::from_bits(mask));
    }
}
