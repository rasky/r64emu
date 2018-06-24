#![feature(attr_literals)]

extern crate emu;
use emu::bus::be::{Mem, MemFlags, Reg32};
use errors::*;
use std::fs::File;
use std::io::Read;

#[derive(DeviceBE, Default)]
pub struct Pi {
    #[mem(bank = 1, offset = 0x0, vsize = 0x7C0)]
    rom: Mem,

    #[mem(bank = 1, offset = 0x7C0, size = 0x40, vsize = 0x40)]
    ram: Mem,

    #[reg(bank = 0, offset = 0x00, rwmask = 0x00FF_FFFF)]
    dma_ram_addr: Reg32,

    #[reg(bank = 0, offset = 0x04, rwmask = 0x00FF_FFFF)]
    dma_rom_addr: Reg32,

    #[reg(bank = 0, offset = 0x08, rwmask = 0x00FF_FFFF, wcb)]
    dma_rd_len: Reg32,

    #[reg(bank = 0, offset = 0x0C, rwmask = 0x00FF_FFFF, wcb)]
    dma_wr_len: Reg32,

    #[reg(bank = 0, offset = 0x10, rwmask = 0, wcb)]
    dma_status: Reg32,
}

impl Pi {
    pub fn new(pifrom: &str) -> Result<Pi> {
        let mut contents = vec![];
        File::open(pifrom)?.read_to_end(&mut contents)?;

        Ok(Pi {
            rom: Mem::from_buffer(contents, MemFlags::READACCESS),
            ..Default::default()
        })
    }

    fn cb_write_dma_status(&self, old: u32, new: u32) {
        unimplemented!()
    }

    fn cb_write_dma_wr_len(&self, old: u32, new: u32) {
        unimplemented!()
    }

    fn cb_write_dma_rd_len(&self, old: u32, new: u32) {
        unimplemented!()
    }
}
