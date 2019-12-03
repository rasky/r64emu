extern crate bit_field;
extern crate byteorder;
extern crate emu;
extern crate slog;
use super::mi::{IrqMask, Mi};
use super::r4300::R4300;
use super::rdp::Rdp;
use super::sp::RSPCPU;
use emu::bus::be::{Device, MemIoR, Reg32, RegDeref, RegRef};
use emu::dbg;
use emu::int::Numerics;
use emu::sync;

bitflags! {
    struct StatusFlags: u32 {
        const XBUS_DMA = 1<<0;
        const FREEZE = 1<<1;
        const FLUSH = 1<<2;
        const START_GLK = 1<<3;
        const TMEM_BUSY = 1<<4;
        const PIPE_BUSY = 1<<5;
        const CMD_BUSY = 1<<6;
        const CMDBUF_BUSY = 1<<7;
        const DMA_BUSY = 1<<8;
        const END_VALID = 1<<9;
        const START_VALID = 1<<10;
    }
}

impl RegDeref for StatusFlags {
    type Type = u32;
    fn from(v: u32) -> StatusFlags {
        StatusFlags::from_bits_truncate(v)
    }
    fn to(&self) -> u32 {
        self.bits()
    }
}

#[derive(DeviceBE)]
pub struct Dp {
    #[reg(bank = 0, offset = 0x0, rwmask = 0x00FFFFFF, wcb)]
    cmd_start: Reg32,

    #[reg(bank = 0, offset = 0x4, rwmask = 0x00FFFFFF, wcb)]
    cmd_end: Reg32,

    #[reg(bank = 0, offset = 0x8, readonly)]
    cmd_current: Reg32,

    #[reg(bank = 0, offset = 0xC, wcb)]
    cmd_status: Reg32,

    logger: slog::Logger,

    fetched_mem: MemIoR<u64>,
    fetched_start_addr: u32,
    fetched_end_addr: u32,
    cycles: i64,
    running: bool,

    gfx: Box<Rdp>,
}

impl Dp {
    pub fn new(logger: slog::Logger) -> Box<Dp> {
        let gfx_logger = logger.new(o!());
        Box::new(Dp {
            cmd_start: Reg32::default(),
            cmd_end: Reg32::default(),
            cmd_current: Reg32::default(),
            cmd_status: Reg32::default(),
            logger,
            cycles: 0,
            running: false,
            fetched_mem: MemIoR::default(),
            fetched_start_addr: 0,
            fetched_end_addr: 0,
            gfx: Box::new(Rdp::new(gfx_logger)),
        })
    }

    fn cmd_status_ref(&self) -> RegRef<StatusFlags> {
        self.cmd_status.as_ref::<StatusFlags>()
    }
    fn cmd_current_ref(&self) -> RegRef<u32> {
        self.cmd_current.as_ref::<u32>()
    }

    fn cb_write_cmd_start(&mut self, _old: u32, _new: u32) {
        self.cmd_status
            .as_ref::<StatusFlags>()
            .insert(StatusFlags::START_VALID);
    }

    fn cb_write_cmd_end(&mut self, _old: u32, _new: u32) {
        self.cmd_status
            .as_ref::<StatusFlags>()
            .insert(StatusFlags::END_VALID);
        self.check_start();
    }

    fn cb_write_cmd_status(&mut self, old: u32, new: u32) {
        self.cmd_status.set(old);
        let mut status = self.cmd_status_ref();
        warn!(self.logger, "writing to DP status"; o!("val" => new.hex()));
        if new & (1<<0) != 0 {
            status.remove(StatusFlags::XBUS_DMA);
        }
        if new & (1<<1) != 0 {
            status.insert(StatusFlags::XBUS_DMA);
        }
    }

    fn check_start(&mut self) {
        let mut status = self.cmd_status_ref();
        if !status.contains(StatusFlags::END_VALID) {
            // if there's no pending end ptr, there's nothing to do.
            return;
        }

        // See if the start ptr changed, if so we need to refetch it.
        // Otherwise, continue from current pointer.
        if status.contains(StatusFlags::START_VALID) {
            let start = self.cmd_start.get();
            *self.cmd_current_ref() = start;
            self.fetched_start_addr = start;
            if status.contains(StatusFlags::XBUS_DMA) {
                self.fetched_mem = RSPCPU::get().bus.fetch_read::<u64>(start);
            } else {
                self.fetched_mem = R4300::get().bus.fetch_read::<u64>(start);
            }
            if self.fetched_mem.iter().is_none() {
                error!(self.logger, "cmd buffer pointing to non-linear memory"; o!("ptr" => start.hex()));
            }
            status.remove(StatusFlags::START_VALID);
        }

        self.fetched_end_addr = self.cmd_end.get();
        status.remove(StatusFlags::END_VALID);
        self.running = true;
        warn!(
            self.logger,
            "DP start";
            o!("start" => self.fetched_start_addr.hex(), "end" => self.fetched_end_addr.hex())
        );
    }
}

impl sync::Subsystem for Dp {
    fn name(&self) -> &str {
        "RDP"
    }

    fn run(&mut self, until: i64, _: &dbg::Tracer) -> dbg::Result<()> {
        if !self.running {
            self.cycles = until;
            return Ok(());
        }
        loop {
            let mut curr_addr = self.cmd_current_ref();
            for cmd in self
                .fetched_mem
                .iter()
                .unwrap()
                .skip((*curr_addr - self.fetched_start_addr) as usize / 8)
                .take((self.fetched_end_addr - *curr_addr) as usize / 8)
            {
                self.gfx.op(cmd);
                *curr_addr += 8;
                self.cycles += 1;
                if self.cycles >= until {
                    return Ok(());
                }
            }

            // Finished the current buffer: stop iteration, but
            // check if there's a new buffer pending
            self.running = false;
            self.check_start();
            if !self.running {
                self.cycles = until;
                Mi::get_mut().set_irq_line(IrqMask::DP, true);
                return Ok(());
            }
        }
    }

    fn step(&mut self, t: &dbg::Tracer) -> dbg::Result<()> {
        self.run(self.cycles + 1, t)
    }

    fn cycles(&self) -> i64 {
        self.cycles
    }

    fn pc(&self) -> Option<u64> {
        None
    }
}
