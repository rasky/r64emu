extern crate emu;
extern crate slog;
use emu::bus::be::Reg32;

#[derive(DeviceBE)]
pub struct Si {
    #[reg(bank = 0, offset = 0x18, rwmask = 0, wcb)]
    status: Reg32,

    logger: slog::Logger,
}

impl Si {
    pub fn new(logger: slog::Logger) -> Si {
        Si {
            status: Reg32::default(),
            logger,
        }
    }

    fn cb_write_status(&self, old: u32, new: u32) {
        unimplemented!();
    }
}
