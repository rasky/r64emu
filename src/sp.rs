extern crate emu;
extern crate slog;

use emu::bus::be::{Mem, Reg32};

#[derive(DeviceBE)]
pub struct Sp {
    #[mem(bank = 1, offset = 0x0000, size = 4096)]
    dmem: Mem,

    #[mem(bank = 1, offset = 0x1000, size = 4096)]
    imem: Mem,

    #[reg(offset = 0x10, init = 0x1, rwmask = 0, wcb)]
    reg_status: Reg32,

    #[reg(offset = 0x18, init = 0, rwmask = 0x1, readonly)]
    reg_dma_busy: Reg32,

    logger: slog::Logger,
}

impl Sp {
    pub fn new(logger: slog::Logger) -> Sp {
        Sp {
            logger,
            dmem: Mem::default(),
            imem: Mem::default(),
            reg_status: Reg32::default(),
            reg_dma_busy: Reg32::default(),
        }
    }

    fn cb_write_reg_status(&self, old: u32, new: u32) {
        info!(self.logger, "write status reg"; o!("val" => format!("{:x}", new)));
    }
}
