extern crate emu;
extern crate slog;
use emu::bus::be::{Bus, Mem, MemFlags, Reg32};
use emu::int::Numerics;
use errors::*;
use std::cell::RefCell;
use std::fs::File;
use std::io::Read;
use std::rc::Rc;

#[derive(DeviceBE)]
pub struct Pi {
    #[mem(bank = 1, offset = 0x0, vsize = 0x7C0)]
    rom: Mem,

    #[mem(bank = 1, offset = 0x7C0, size = 0x40, vsize = 0x3C)]
    ram: Mem,

    #[reg(bank = 1, offset = 0x7FC, wcb, rcb)]
    magic: Reg32,

    // [23:0] starting RDRAM address
    #[reg(bank = 0, offset = 0x00, rwmask = 0x00FF_FFFF)]
    dma_ram_addr: Reg32,

    // [31:0] starting AD16 address
    #[reg(bank = 0, offset = 0x04)]
    dma_rom_addr: Reg32,

    // [23:0] read data length
    #[reg(bank = 0, offset = 0x08, rwmask = 0x00FF_FFFF, wcb)]
    dma_rd_len: Reg32,

    // [23:0] write data length
    #[reg(bank = 0, offset = 0x0C, rwmask = 0x00FF_FFFF, wcb)]
    dma_wr_len: Reg32,

    // (R) [0] DMA busy             (W): [0] reset controller
    //     [1] IO busy                       (and abort current op)
    //     [2] error [1] clear intr
    #[reg(bank = 0, offset = 0x10, rwmask = 0, wcb)]
    dma_status: Reg32,

    // [7:0] domain 1 device latency
    #[reg(bank = 0, offset = 0x0014, rwmask = 0)]
    dom1_latency: Reg32,

    // [7:0] domain 1 device R/W strobe pulse width
    #[reg(bank = 0, offset = 0x0018, rwmask = 0)]
    dom1_pulse_width: Reg32,

    // [3:0] domain 1 device page size
    #[reg(bank = 0, offset = 0x001C, rwmask = 0xF)]
    dom1_page_size: Reg32,

    // [1:0] domain 1 device R/W release duration
    #[reg(bank = 0, offset = 0x0020, rwmask = 0x3)]
    dom1_release: Reg32,

    // [7:0] domain 2 device latency
    #[reg(bank = 0, offset = 0x0024, rwmask = 0xFF)]
    dom2_latency: Reg32,

    // [7:0] domain 2 device R/W strobe pulse width
    #[reg(bank = 0, offset = 0x0028, rwmask = 0xFF)]
    dom2_pulse_width: Reg32,

    // [3:0] domain 2 device page size
    #[reg(bank = 0, offset = 0x002C, rwmask = 0xF)]
    dom2_page_size: Reg32,

    // [1:0] domain 2 device R/W release duration
    #[reg(bank = 0, offset = 0x0030, rwmask = 0x3)]
    dom2_release: Reg32,

    logger: slog::Logger,
    bus: Rc<RefCell<Box<Bus>>>,
}

impl Pi {
    pub fn new(logger: slog::Logger, bus: Rc<RefCell<Box<Bus>>>, pifrom: &str) -> Result<Pi> {
        let mut contents = vec![];
        File::open(pifrom)?.read_to_end(&mut contents)?;

        Ok(Pi {
            logger,
            bus,
            rom: Mem::from_buffer(contents, MemFlags::READACCESS),
            ram: Mem::default(),
            magic: Reg32::default(),
            dma_ram_addr: Reg32::default(),
            dma_rom_addr: Reg32::default(),
            dma_rd_len: Reg32::default(),
            dma_wr_len: Reg32::default(),
            dma_status: Reg32::default(),
            dom1_latency: Reg32::default(),
            dom1_pulse_width: Reg32::default(),
            dom1_page_size: Reg32::default(),
            dom1_release: Reg32::default(),
            dom2_latency: Reg32::default(),
            dom2_pulse_width: Reg32::default(),
            dom2_page_size: Reg32::default(),
            dom2_release: Reg32::default(),
        })
    }

    fn cb_read_magic(&self, val: u32) -> u32 {
        info!(self.logger, "read magic"; o!("val" => format!("{:x}", val)));
        val
    }

    fn cb_write_magic(&mut self, _old: u32, new: u32) {
        if new & 0x20 != 0 {
            info!(self.logger, "magic: unlock boot");
            self.magic.set(self.magic.get() | 0x80);
        }
    }

    fn cb_write_dma_status(&mut self, _old: u32, new: u32) {
        info!(self.logger, "write dma status"; o!("val" => format!("{:x}", new)));
    }

    fn cb_write_dma_wr_len(&mut self, _old: u32, val: u32) {
        let mut raddr = self.dma_rom_addr.get();
        let mut waddr = self.dma_ram_addr.get();
        info!(self.logger, "DMA xfer"; o!(
            "src" => raddr.hex(),
            "dst" => waddr.hex(),
            "len" => val+1));

        let bus = self.bus.borrow();
        let mut i = 0;
        while i < val + 1 {
            bus.write::<u32>(waddr, bus.read::<u32>(raddr));
            raddr = raddr + 4;
            waddr = waddr + 4;
            i += 4;
        }
        self.dma_rom_addr.set(raddr);
        self.dma_ram_addr.set(waddr);
    }

    fn cb_write_dma_rd_len(&mut self, _old: u32, _new: u32) {
        unimplemented!()
    }
}
