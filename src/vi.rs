extern crate emu;
extern crate slog;
use emu::bus::be::Reg32;
use emu::int::Numerics;

#[derive(DeviceBE)]
pub struct Vi {
    #[reg(bank = 0, offset = 0x10, rwmask = 0, wcb)]
    current_line: Reg32,

    logger: slog::Logger,
}

impl Vi {
    pub fn new(logger: slog::Logger) -> Vi {
        Vi {
            current_line: Reg32::default(),
            logger,
        }
    }

    fn cb_write_current_line(&self, _old: u32, new: u32) {
        error!(self.logger, "write VI current line"; o!("val" => new.hex()));
    }
}
