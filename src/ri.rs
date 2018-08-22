extern crate emu;
extern crate slog;
use emu::bus::be::{Mem, Reg32};
use emu::int::Numerics;

/// RDRAM interface (RI)
#[derive(DeviceBE)]
pub struct Ri {
    #[mem(
        bank = 0,
        size = 4194304,
        offset = 0x0000_0000,
        vsize = 0x03F0_0000
    )]
    rdram: Mem,

    // [1:0] operating mode
    // [2] stop T active
    // [3] stop R active
    #[reg(bank = 1, offset = 0x00, rwmask = 0xF)]
    mode: Reg32,

    // [5:0] current control input
    // [6] current control enable
    #[reg(bank = 1, offset = 0x04, rwmask = 0x3F)]
    config: Reg32,

    // (W): [] any write updates current control register
    #[reg(bank = 1, offset = 0x08, writeonly)]
    current_load: Reg32,

    // [2:0] receive select
    // [2:0] transmit select
    #[reg(bank = 1, offset = 0x0C, rwmask = 0xF)]
    select: Reg32,

    // [7:0] clean refresh delay
    // [15:8] dirty refresh delay
    // [16] refresh bank
    // [17] refresh enable
    // [18] refresh optimize
    #[reg(bank = 1, offset = 0x10, rwmask = 0x7FFFF)]
    refresh: Reg32,

    // [3:0] DMA latency/overlap
    #[reg(bank = 1, offset = 0x14, rwmask = 0xF)]
    latency: Reg32,

    // (R): [0] nack error
    //      [1] ack error
    #[reg(bank = 1, offset = 0x18, rwmask = 0x2, readonly)]
    error: Reg32,

    // (W): [] any write clears all error bits
    #[reg(bank = 1, offset = 0x1C, writeonly)]
    error_write: Reg32,

    logger: slog::Logger,
}

impl Ri {
    pub fn new(logger: slog::Logger) -> Ri {
        Ri {
            rdram: Mem::default(),
            mode: Reg32::default(),
            config: Reg32::default(),
            current_load: Reg32::default(),
            select: Reg32::default(),
            refresh: Reg32::default(),
            latency: Reg32::default(),
            error: Reg32::default(),
            error_write: Reg32::default(),
            logger,
        }
    }
}
