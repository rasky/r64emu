use super::mi::{IrqMask, Mi};
use emu::bus::be::{Device, Reg32};
use emu::dbg;
use emu::int::Numerics;
use emu::snd::{SampleFormat, SndBufferMut};
use emu::state::{ArrayField, Field};
use emu::sync;
use emu_derive::DeviceBE;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
struct AudioFifo {
    src: u32,   // Source RDRAM address of sample data
    len: u32,   // Source length of sample data in bytes
    full: bool, // True if this AudioFifo is full (not empty)
}

#[derive(DeviceBE)]
pub struct Ai {
    // (W): [23:0] starting RDRAM address (8B-aligned)
    #[reg(bank = 0, offset = 0x00, rwmask = 0xFFFFFF, writeonly)]
    reg_dram_address: Reg32,

    // [14:0] transfer length (v1.0) - Bottom 3 bits are ignored
    // [17:0] transfer length (v2.0) - Bottom 3 bits are ignored
    #[reg(bank = 0, offset = 0x04, rwmask = 0x3FFF8, wcb)]
    reg_length: Reg32,

    // (W): [0] DMA enable - if LSB == 1, DMA is enabled
    #[reg(bank = 0, offset = 0x08, rwmask = 0x1, writeonly, wcb)]
    reg_control: Reg32,

    // (R): [31]/[0] ai_full (addr & len buffer full)
    //      [30] ai_busy
    //      Note that a 1to0 transition in ai_full will set interrupt
    // (W): clear audio interrupt
    #[reg(bank = 0, offset = 0x0C, wcb)]
    reg_status: Reg32,

    // (W): [13:0] dac rate
    //           - vid_clock/(dperiod + 1) is the DAC sample rate
    //           - (dperiod + 1) >= 66 * (aclockhp + 1) must be true
    #[reg(bank = 0, offset = 0x10, rwmask = 0x3FFF, writeonly)]
    reg_dac_sample_period: Reg32,

    // (W): [3:0] bit rate (abus clock half period register - aclockhp)
    //          - vid_clock/(2*(aclockhp + 1)) is the DAC clock rate
    //          - The abus clock stops if aclockhp is zero
    #[reg(bank = 0, offset = 0x14, rwmask = 0xF, writeonly)]
    reg_bit_rate: Reg32,

    fifo: ArrayField<AudioFifo>,
    fifo_cur: Field<usize>,
    cycles: Field<i64>,

    logger: slog::Logger,
}

impl Ai {
    pub fn new(logger: slog::Logger) -> Box<Ai> {
        Box::new(Ai {
            reg_dram_address: Reg32::default(),
            reg_length: Reg32::default(),
            reg_control: Reg32::default(),
            reg_status: Reg32::default(),
            reg_dac_sample_period: Reg32::default(),
            reg_bit_rate: Reg32::default(),
            fifo: ArrayField::new("Ai::fifo", AudioFifo::default(), 2),
            fifo_cur: Field::new("Ai::fifo_cur", 0),
            cycles: Field::new("Ai::cycles", 0),
            logger,
        })
    }

    fn update_status(&mut self) {
        let mut status = self.reg_status.as_ref::<u32>();
        if self.fifo[0].full && self.fifo[1].full {
            *status |= 1 << 31;
        } else {
            if *status & (1 << 31) != 0 {
                // 1-to-0 transition of full bit triggers an interrupt
                Mi::get_mut().set_irq_line(IrqMask::AI, true);
                info!(self.logger, "audio fifo slot available, trigger IRQ");
            }
            *status &= !(1 << 31);
        }
        if self.fifo[0].full || self.fifo[1].full {
            *status |= 1 << 30;
        } else {
            *status &= !(1 << 30);
            info!(self.logger, "DMA finished");
        }
    }

    fn cb_write_reg_length(&mut self, _old: u32, _new: u32) {
        let src = self.reg_dram_address.get();
        let len = self.reg_length.get();

        // Do not start DMA when len==0. TODO: verify
        if len == 0 {
            return;
        }

        let mut widx = *self.fifo_cur;
        if self.fifo[widx].full {
            widx ^= 1;
            if self.fifo[widx].full {
                error!(self.logger, "audio fifo overflow");
                return;
            }
        }

        info!(self.logger, "start DMA"; "src" => src.hex(), "len" => len);
        self.fifo[widx] = AudioFifo {
            src,
            len,
            full: true,
        };
        self.update_status();
    }

    fn cb_write_reg_control(&self, _old: u32, new: u32) {
        info!(self.logger, "written reg_control"; "val" => new.hex());
    }

    fn cb_write_reg_status(&mut self, old: u32, _new: u32) {
        self.reg_status.set(old);
        Mi::get_mut().set_irq_line(IrqMask::AI, false);
        info!(self.logger, "IRQ acknowledge");
    }

    pub fn play_frame<SF: SampleFormat>(&mut self, _sound: &mut SndBufferMut<SF>) {}
}

impl sync::Subsystem for Ai {
    fn name(&self) -> &str {
        "Ai"
    }

    fn run(&mut self, target_cycles: i64, _tracer: &dbg::Tracer) -> dbg::Result<()> {
        debug!(self.logger, "AI run";
            "fifo" => ?self.fifo[*self.fifo_cur],
            "other" => ?self.fifo[*self.fifo_cur^1],
            "cur" => *self.fifo_cur);
        let audioframe_size = (self.reg_bit_rate.get() + 1) / 8 * 2;
        while *self.cycles < target_cycles {
            let fifo = &mut self.fifo[*self.fifo_cur];
            if fifo.full {
                fifo.src += audioframe_size;
                fifo.len = fifo.len.checked_sub(audioframe_size).unwrap();

                if fifo.len == 0 {
                    fifo.full = false;
                    *self.fifo_cur ^= 1;
                }
            }
            *self.cycles += self.reg_dac_sample_period.get() as i64 + 1;
        }

        // Update also user-visibile registers (that reflect current FIFO)
        let fifo = &self.fifo[*self.fifo_cur];
        self.reg_dram_address.set(fifo.src);
        self.reg_length.set(fifo.len);

        self.update_status();
        Ok(())
    }
    fn step(&mut self, _tracer: &dbg::Tracer) -> dbg::Result<()> {
        panic!("Ai::step() should never be called");
    }
    fn cycles(&self) -> i64 {
        *self.cycles
    }
    fn pc(&self) -> Option<u64> {
        None // No program counter
    }
}
