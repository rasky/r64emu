extern crate emu;
extern crate slog;
use emu::bus::be::{Mem, Reg32};
use emu::int::Numerics;

/// RDRAM
#[derive(DeviceBE)]
pub struct Ri {
    #[mem(
        bank = 0,
        size = 4194304,
        offset = 0x0000_0000,
        vsize = 0x03F0_0000,
    )]
    rdram: Mem,

    #[reg(bank = 1, offset = 0x00)]
    reg_rdram_config: Reg32,

    #[reg(bank = 1, offset = 0x04)]
    reg_rdram_device_id: Reg32,

    #[reg(bank = 1, offset = 0x08)]
    reg_rdram_delay: Reg32,

    #[reg(bank = 1, offset = 0x0C)]
    reg_rdram_mode: Reg32,

    #[reg(bank = 1, offset = 0x10)]
    reg_rdram_interval: Reg32,

    #[reg(bank = 1, offset = 0x14)]
    reg_rdram_ref_row: Reg32,

    #[reg(bank = 1, offset = 0x18)]
    reg_rdram_ras_interval: Reg32,

    #[reg(bank = 1, offset = 0x1C)]
    reg_rdram_min_interval: Reg32,

    #[reg(bank = 1, offset = 0x20)]
    reg_rdram_addr_select: Reg32,

    #[reg(bank = 1, offset = 0x24)]
    reg_rdram_device_manuf: Reg32,

    // [1:0] operating mode
    // [2] stop T active
    // [3] stop R active
    #[reg(bank = 2, offset = 0x00, rwmask = 0xF)]
    reg_ri_mode: Reg32,

    // [5:0] current control input
    // [6] current control enable
    #[reg(bank = 2, offset = 0x04, rwmask = 0x3F)]
    reg_ri_config: Reg32,

    // (W): [] any write updates current control register
    #[reg(bank = 2, offset = 0x08, writeonly)]
    reg_ri_current_load: Reg32,

    // [2:0] receive select
    // [2:0] transmit select
    #[reg(bank = 2, offset = 0x0C, rwmask = 0xF)]
    reg_ri_select: Reg32,

    // [7:0] clean refresh delay
    // [15:8] dirty refresh delay
    // [16] refresh bank
    // [17] refresh enable
    // [18] refresh optimize
    #[reg(bank = 2, offset = 0x10, rwmask = 0x7FFFF)]
    reg_ri_refresh: Reg32,

    // [3:0] DMA latency/overlap
    #[reg(bank = 2, offset = 0x14, rwmask = 0xF)]
    reg_ri_latency: Reg32,

    // (R): [0] nack error
    //      [1] ack error
    #[reg(bank = 2, offset = 0x18, rwmask = 0x2, readonly)]
    reg_ri_error: Reg32,

    // (W): [] any write clears all error bits
    #[reg(bank = 2, offset = 0x1C, writeonly)]
    reg_ri_error_write: Reg32,

    logger: slog::Logger,
}

impl Ri {
    pub fn new(logger: slog::Logger) -> Ri {
        let ri = Ri {
            rdram: Mem::default(),

            reg_rdram_config: Reg32::default(),
            reg_rdram_device_id: Reg32::default(),
            reg_rdram_delay: Reg32::default(),
            reg_rdram_mode: Reg32::default(),
            reg_rdram_interval: Reg32::default(),
            reg_rdram_ref_row: Reg32::default(),
            reg_rdram_ras_interval: Reg32::default(),
            reg_rdram_min_interval: Reg32::default(),
            reg_rdram_addr_select: Reg32::default(),
            reg_rdram_device_manuf: Reg32::default(),

            reg_ri_mode: Reg32::default(),
            reg_ri_config: Reg32::default(),
            reg_ri_current_load: Reg32::default(),
            reg_ri_select: Reg32::default(),
            reg_ri_refresh: Reg32::default(),
            reg_ri_latency: Reg32::default(),
            reg_ri_error: Reg32::default(),
            reg_ri_error_write: Reg32::default(),

            logger,
        };

        // defaults from cen64
        ri.reg_ri_mode.set(0xE);
        ri.reg_ri_config.set(0x40);
        ri.reg_ri_select.set(0x14);
        ri.reg_ri_refresh.set(0x63634);

        ri
    }
}
