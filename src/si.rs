extern crate emu;
extern crate slog;
use emu::bus::be::Reg32;
use emu::int::Numerics;
use emu_derive::DeviceBE;

#[derive(DeviceBE)]
pub struct Si {
    #[reg(bank = 0, offset = 0x18, rwmask = 0, rcb, wcb)]
    status: Reg32,

    logger: slog::Logger,
}

impl Si {
    pub fn new(logger: slog::Logger) -> Box<Si> {
        Box::new(Si {
            status: Reg32::default(),
            logger,
        })
    }

    fn cb_read_status(&self, val: u32) -> u32 {
        error!(self.logger, "read SI status reg"; "val" => val.hex());
        val
    }

    fn cb_write_status(&self, _old: u32, new: u32) {
        error!(self.logger, "write SI status reg"; "val" => new.hex());
    }
}
