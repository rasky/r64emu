use slog;

use super::mi::{IrqMask, Mi};
use super::r4300::R4300;
use super::pi::Pi;

use emu::bus::be::Reg32;
use emu::bus::Device;
use emu::int::Numerics;
use emu_derive::DeviceBE;

#[derive(DeviceBE)]
pub struct Si {
    #[reg(bank = 0, offset = 0x00)]
    dma_address: Reg32,

    #[reg(bank = 0, offset = 0x04, writeonly, wcb)]
    start_dma_read: Reg32,

    #[reg(bank = 0, offset = 0x10, writeonly, wcb)]
    start_dma_write: Reg32,

    // (W): [] any write clears interrupt
    // (R): [0] DMA busy
    //      [1] IO read busy
    //      [2] reserved
    //      [3] DMA error
    //      [12] interrupt
    #[reg(bank = 0, offset = 0x18, rwmask = 0, wcb)]
    status: Reg32,

    logger: slog::Logger,
}

impl Si {
    pub fn new(logger: slog::Logger) -> Box<Si> {
        Box::new(Si {
            status: Reg32::default(),
            dma_address: Reg32::default(),
            start_dma_read: Reg32::default(),
            start_dma_write: Reg32::default(),
            logger,
        })
    }

    pub(crate) fn set_busy(&mut self, busy: bool) {
        let mut status = self.status.get();
        if busy {
            status |= 1 << 1;
        } else {
            status &= !(1 << 1);
        }
        self.status.set(status);
    }

    pub(crate) fn raise_irq(&mut self) {
        let status = self.status.get();
        self.status.set(status | (1 << 12));
        Mi::get_mut().set_irq_line(IrqMask::SI, true);
    }

    fn cb_write_status(&mut self, old: u32, new: u32) {
        // Any write to SI status clears the IRQ line
        self.status.set(old & !(1 << 12));
        Mi::get_mut().set_irq_line(IrqMask::SI, false);

        info!(self.logger, "write SI status reg"; "val" => new.hex());
    }

    fn cb_write_start_dma_read(&mut self, _old: u32, new: u32) {
        let mut src = new;
        let mut dst = self.dma_address.get();
        info!(self.logger, "SI DMA read"; "pifram" => src.hex(), "rdram" => dst.hex());

        let bus = &mut R4300::get_mut().bus;
        for _ in 0..16 {
            let val = bus.read::<u32>(src);
            bus.write::<u32>(dst, val);
            src += 4;
            dst += 4;
        }
        self.raise_irq();
    }

    fn cb_write_start_dma_write(&mut self, _old: u32, new: u32) {
        let mut src = self.dma_address.get();
        let mut dst = new;
        info!(self.logger, "SI DMA write"; "rdram" => src.hex(), "pifram" => dst.hex());

        let bus = &mut R4300::get_mut().bus;
        for _ in 0..16 {
            let val = bus.read::<u32>(src);
            bus.write::<u32>(dst, val);
            src += 4;
            dst += 4;
        }
        self.raise_irq();

        if bus.read::<u8>(0x1fc0_07c0) & 1 != 0 {
            self.set_busy(true);
        }

        // Dump PIF RAM
        let dst = bus.fetch_read::<u8>(new);
        let mut mem = dst.iter().unwrap();
        for i in 0..8 {
            println!(
                "SI: {:03x}: {:02x} {:02x} {:02x} {:02x} -- {:02x} {:02x} {:02x} {:02x}",
                (new & 0xFFF) + i * 8,
                mem.next().unwrap(),
                mem.next().unwrap(),
                mem.next().unwrap(),
                mem.next().unwrap(),
                mem.next().unwrap(),
                mem.next().unwrap(),
                mem.next().unwrap(),
                mem.next().unwrap()
            );
        }
    }
}
