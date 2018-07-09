extern crate emu;
extern crate slog;
use emu::bus::be::Reg32;
use emu::int::Numerics;

#[derive(DeviceBE)]
pub struct Ai {
    #[reg(bank = 0, offset = 0xC, wcb)]
    status: Reg32,

    logger: slog::Logger,
}

impl Ai {
    pub fn new(logger: slog::Logger) -> Ai {
        Ai {
            status: Reg32::default(),
            logger,
        }
    }

    fn cb_write_status(&self, _old: u32, new: u32) {
        error!(self.logger, "write AI status"; o!("val" => new.hex()));
    }
}
