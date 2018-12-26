use emu::bus::be::Reg32;
use emu::int::Numerics;
use emu_derive::DeviceBE;

#[derive(DeviceBE)]
pub struct Ai {
    // (W): [23:0] starting RDRAM address (8B-aligned)
    #[reg(bank = 0, offset = 0x00, rwmask = 0xFFFFFF, writeonly)]
    dram_address: Reg32,

    // [14:0] transfer length (v1.0) - Bottom 3 bits are ignored
    // [17:0] transfer length (v2.0) - Bottom 3 bits are ignored
    #[reg(bank = 0, offset = 0x04, rwmask = 0x3FFFF)]
    length: Reg32,

    // (W): [0] DMA enable - if LSB == 1, DMA is enabled
    #[reg(bank = 0, offset = 0x08, rwmask = 0x1, writeonly)]
    control: Reg32,

    // (R): [31]/[0] ai_full (addr & len buffer full)
    //      [30] ai_busy
    //      Note that a 1to0 transition in ai_full will set interrupt
    // (W): clear audio interrupt
    #[reg(bank = 0, offset = 0x0C, wcb)]
    status: Reg32,

    // (W): [13:0] dac rate
    //           - vid_clock/(dperiod + 1) is the DAC sample rate
    //           - (dperiod + 1) >= 66 * (aclockhp + 1) must be true
    #[reg(bank = 0, offset = 0x10, rwmask = 0x3FFF, writeonly)]
    dac_sample_period: Reg32,

    // (W): [3:0] bit rate (abus clock half period register - aclockhp)
    //          - vid_clock/(2*(aclockhp + 1)) is the DAC clock rate
    //          - The abus clock stops if aclockhp is zero
    #[reg(bank = 0, offset = 0x14, rwmask = 0xF, writeonly)]
    bit_rate: Reg32,

    logger: slog::Logger,
}

impl Ai {
    pub fn new(logger: slog::Logger) -> Box<Ai> {
        Box::new(Ai {
            dram_address: Reg32::default(),
            length: Reg32::default(),
            control: Reg32::default(),
            status: Reg32::default(),
            dac_sample_period: Reg32::default(),
            bit_rate: Reg32::default(),
            logger,
        })
    }

    fn cb_write_status(&self, _old: u32, new: u32) {
        error!(self.logger, "write AI status"; o!("val" => new.hex()));
    }
}
