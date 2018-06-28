extern crate emu;
extern crate slog;
use emu::bus::be::Reg32;
use emu::int::Numerics;

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

    fn cb_write_status(&self, _old: u32, new: u32) {
        error!(self.logger, "write SI status reg"; o!("val" => new.hex()));
    }
}
