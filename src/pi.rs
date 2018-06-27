extern crate emu;
extern crate slog;
use emu::bus::be::{Mem, MemFlags, Reg32};
use errors::*;
use std::fs::File;
use std::io::Read;

#[derive(DeviceBE)]
pub struct Pi {
    #[mem(bank = 1, offset = 0x0, vsize = 0x7C0)]
    rom: Mem,

    #[mem(bank = 1, offset = 0x7C0, size = 0x40, vsize = 0x3C)]
    ram: Mem,

    #[reg(bank = 1, offset = 0x7FC, wcb, rcb)]
    magic: Reg32,

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

    logger: slog::Logger,
}

impl Pi {
    pub fn new(logger: slog::Logger, pifrom: &str) -> Result<Pi> {
        let mut contents = vec![];
        File::open(pifrom)?.read_to_end(&mut contents)?;

        Ok(Pi {
            logger,
            rom: Mem::from_buffer(contents, MemFlags::READACCESS),
            ram: Mem::default(),
            magic: Reg32::default(),
            dma_ram_addr: Reg32::default(),
            dma_rom_addr: Reg32::default(),
            dma_rd_len: Reg32::default(),
            dma_wr_len: Reg32::default(),
            dma_status: Reg32::default(),
        })
    }

    fn cb_read_magic(&self, val: u32) -> u32 {
        info!(self.logger, "read magic"; o!("val" => format!("{:x}", val)));
        val
    }

    fn cb_write_magic(&mut self, old: u32, new: u32) {
        if new & 0x20 != 0 {
            crit!(self.logger, "magic: unlock boot");
            self.magic.set(self.magic.get() | 0x80);
            panic!("ciaos");
        }
    }

    fn cb_write_dma_status(&mut self, _old: u32, new: u32) {
        info!(self.logger, "write dma status"; o!("val" => format!("{:x}", new)));
    }

    fn cb_write_dma_wr_len(&mut self, old: u32, new: u32) {
        unimplemented!()
    }

    fn cb_write_dma_rd_len(&mut self, old: u32, new: u32) {
        unimplemented!()
    }
}
