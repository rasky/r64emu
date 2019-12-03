use super::r4300::R4300;
use emu::bus::be::{Device, Reg32};
use emu::int::Numerics;
use mips64::Cop0;

use bit_field::BitField;
use bitflags::bitflags;
use slog;

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
    // 0x04300000 to 0x04300003  MI_INIT_MODE_REG or MI_MODE_REG //MI init mode
    // (W): [0-6] init length        (R): [0-6] init length
    //  [7] clear init mode           [7] init mode
    //  [8] set init mode             [8] ebus test mode
    //  [9/10] clr/set ebus test mode [9] RDRAM reg mode
    //  [11] clear DP interrupt
    //  [12] clear RDRAM reg
    //  [13] set RDRAM reg mode
    #[reg(offset = 0x00, wcb)]
    reg_mode: Reg32,

    #[reg(offset = 0x04, init = 0x02020102, readonly)]
    reg_version: Reg32,

    #[reg(offset = 0x08, readonly)]
    irq_ack: Reg32,

    #[reg(offset = 0x0C, wcb)]
    irq_mask: Reg32,

    logger: slog::Logger,
}

impl Mi {
    pub fn new(logger: slog::Logger) -> Box<Mi> {
        Box::new(Mi {
            reg_mode: Reg32::default(),
            irq_ack: Reg32::default(),
            irq_mask: Reg32::default(),
            reg_version: Reg32::default(),
            logger,
        })
    }

    fn cb_write_reg_mode(&mut self, old: u32, new: u32) {
        let mut mode = old;

        mode.set_bits(0..7, new.get_bits(0..7));

        if new.get_bit(7) {
            // clear init mode
            mode.set_bit(7, false);
        }
        if new.get_bit(8) {
            // set init mode
            mode.set_bit(7, true);
        }
        if new.get_bit(9) {
            // clear ebus
            mode.set_bit(8, false);
        }
        if new.get_bit(10) {
            // set ebus
            mode.set_bit(8, true);
        }
        if new.get_bit(11) {
            // clear RDP interrupt
            self.set_irq_line(IrqMask::DP, false);
        }
        if new.get_bit(12) {
            // clear ebus
            mode.set_bit(9, false);
        }
        if new.get_bit(13) {
            // set ebus
            mode.set_bit(9, true);
        }
        self.reg_mode.set(mode);
        info!(self.logger, "written reg_mode"; "mode" => mode.hex());
    }

    pub fn set_irq_line(&mut self, lines: IrqMask, status: bool) {
        let old = self.irq_ack.get();
        let new = if status {
            old | lines.bits()
        } else {
            old & !lines.bits()
        };
        self.irq_ack.set(new);

        if old != new {
            info!(self.logger, "changed IRQ ack"; "irq" => ?IrqMask::from_bits(new));
        }
        self.update_cpu_irq();
    }

    fn cb_write_irq_mask(&mut self, old: u32, new: u32) {
        let mut mask = old;
        for i in 0..12 {
            if new.get_bit(i) {
                mask.set_bit(i / 2, i % 2 != 0);
            }
        }
        self.irq_mask.set(mask);
        if old != mask {
            info!(self.logger, "changed IRQ mask"; "irq" => ?IrqMask::from_bits(mask));
        }
        self.update_cpu_irq();
    }

    fn update_cpu_irq(&self) {
        R4300::get_mut()
            .cop0
            .set_hwint_line(0, (self.irq_ack.get() & self.irq_mask.get()) != 0);
    }
}
