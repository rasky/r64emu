extern crate emu;
extern crate slog;

use super::super::dp::Dp;
use super::super::mi::{IrqMask, Mi};
use super::cop0::SpCop0;
use super::cop2::SpCop2;
use crate::errors::*;
use emu::bus::be::{Bus, DevPtr, Mem, Reg32};
use emu::int::Numerics;
use mips64;
use std::cell::RefCell;
use std::rc::Rc;

bitflags! {
    pub(crate) struct StatusFlags: u32 {
        const HALT =             0b_0000_0001;
        const BROKE =            0b_0000_0010;
        const DMABUSY =          0b_0000_0100;
        const DMAFULL =          0b_0000_1000;
        const IOFULL =          0b0_0001_0000;
        const SINGLESTEP =     0b00_0010_0000;
        const INTBREAK =      0b000_0100_0000;
        const SIG0 =        0b_0000_1000_0000;
        const SIG1 =       0b0_0001_0000_0000;
        const SIG2 =      0b00_0010_0000_0000;
        const SIG3 =     0b000_0100_0000_0000;
        const SIG4 =    0b0000_1000_0000_0000;
        const SIG5 =   0b00001_0000_0000_0000;
        const SIG6 =  0b000010_0000_0000_0000;
        const SIG7 = 0b0000100_0000_0000_0000;
    }
}

pub struct RSPCPUConfig;

impl mips64::Config for RSPCPUConfig {
    type Arch = mips64::ArchI; // 32-bit MIPS I architecture
    type Cop0 = SpCop0;
    type Cop1 = mips64::CopNull;
    type Cop2 = SpCop2;
    type Cop3 = mips64::CopNull;
    fn pc_mask(pc: u32) -> u32 {
        (pc & 0xFFF) | 0x1000
    }
    fn addr_mask(addr: u32) -> u32 {
        addr & 0xFFF
    }
}

pub type RSPCPU = mips64::Cpu<RSPCPUConfig>;

#[derive(DeviceBE)]
pub struct Sp {
    pub core_cpu: Option<Rc<RefCell<Box<RSPCPU>>>>, // FIXME: remove after DevPtr refactoring
    pub core_bus: Rc<RefCell<Box<Bus>>>,

    #[mem(bank = 0, offset = 0x0000, size = 4096)]
    pub dmem: Mem,

    #[mem(bank = 0, offset = 0x1000, size = 4096)]
    pub imem: Mem,

    #[reg(bank = 2, offset = 0x0, rwmask = 0xFFF, wcb, rcb)]
    reg_rsp_pc: Reg32,

    #[reg(bank = 1, offset = 0x00, rwmask = 0x1FF8)]
    reg_dma_rsp_addr: Reg32,

    #[reg(bank = 1, offset = 0x04, rwmask = 0xFFFFF8)]
    reg_dma_rdram_addr: Reg32,

    #[reg(bank = 1, offset = 0x08, wcb)]
    reg_dma_rd_len: Reg32,

    #[reg(bank = 1, offset = 0x0C, wcb)]
    reg_dma_wr_len: Reg32,

    #[reg(bank = 1, offset = 0x10, init = 0x1, wcb)]
    reg_status: Reg32,

    #[reg(bank = 1, offset = 0x14, readonly, rcb)]
    reg_dma_full: Reg32,

    #[reg(bank = 1, offset = 0x18, readonly, rcb)]
    reg_dma_busy: Reg32,

    #[reg(bank = 1, offset = 0x1C, init = 0x0, rwmask = 0x1, rcb)]
    reg_semaphore: Reg32,

    logger: slog::Logger,

    main_bus: Rc<RefCell<Box<Bus>>>,
    pub(crate) mi: DevPtr<Mi>,
}

impl Sp {
    pub fn new(
        logger: slog::Logger,
        main_bus: Rc<RefCell<Box<Bus>>>,
        dp: &DevPtr<Dp>,
        mi: DevPtr<Mi>,
    ) -> Result<DevPtr<Sp>> {
        // Create the RSP internal MIPS CPU and its associated bus
        let bus = Rc::new(RefCell::new(Bus::new(logger.new(o!()))));
        let mut sp = DevPtr::new(Sp {
            logger: logger.new(o!()),
            main_bus,
            mi,
            dmem: Mem::default(),
            imem: Mem::default(),
            reg_status: Reg32::default(),
            reg_dma_busy: Reg32::default(),
            reg_dma_rsp_addr: Reg32::default(),
            reg_dma_rdram_addr: Reg32::default(),
            reg_dma_wr_len: Reg32::default(),
            reg_dma_rd_len: Reg32::default(),
            reg_rsp_pc: Reg32::default(),
            reg_dma_full: Reg32::default(),
            reg_semaphore: Reg32::default(),

            core_bus: bus.clone(),
            core_cpu: None,
        });

        let cpu = Rc::new(RefCell::new(Box::new(mips64::Cpu::new(
            "RSP",
            logger.new(o!()),
            bus.clone(),
            (
                SpCop0::new(&sp, dp, logger.new(o!()))?,
                mips64::CopNull {},
                SpCop2::new(&sp, logger.new(o!()))?,
                mips64::CopNull {},
            ),
        ))));

        {
            // Set the CPU into SP.
            // FIXME: remove this once we remove the reference check and thus the Option
            let mut spb = sp.borrow_mut();
            spb.core_cpu = Some(cpu);
        }

        {
            // Configure RSP internal core bus
            let mut bus = bus.borrow_mut();
            bus.map_device(0x0000_0000, &sp, 0)?;
        }

        Ok(sp)
    }

    pub(crate) fn get_status(&self) -> StatusFlags {
        StatusFlags::from_bits(self.reg_status.get()).unwrap()
    }

    fn cb_write_reg_status(&mut self, old: u32, new: u32) {
        self.reg_status.set(old); // restore previous value, as write bits are completely different
        let change_halt = self.write_status(new);

        let mut cpu = self.core_cpu.as_ref().unwrap().borrow_mut();
        match change_halt {
            Some(halt) => cpu.ctx_mut().set_halt_line(halt),
            None => {}
        }
    }

    // Emulate a write the RSP status register. The return value is the same
    // of set_status().
    #[must_use]
    pub(crate) fn write_status(&mut self, writebits: u32) -> Option<bool> {
        let mut status = self.get_status();
        let new = writebits;
        if new & (1 << 0) != 0 {
            status.remove(StatusFlags::HALT);
        }
        if new & (1 << 1) != 0 {
            status.insert(StatusFlags::HALT);
        }
        if new & (1 << 2) != 0 {
            status.remove(StatusFlags::BROKE);
        }
        if new & (1 << 3) != 0 {
            info!(self.logger, "clear RSP Interrupt");
            self.mi.borrow_mut().set_irq_line(IrqMask::SP, false);
        }
        if new & (1 << 4) != 0 {
            info!(self.logger, "force-set RSP Interrupt");
            self.mi.borrow_mut().set_irq_line(IrqMask::SP, true);
        }
        if new & (1 << 5) != 0 {
            status.remove(StatusFlags::SINGLESTEP);
        }
        if new & (1 << 6) != 0 {
            status.insert(StatusFlags::SINGLESTEP);
        }
        if new & (1 << 7) != 0 {
            status.remove(StatusFlags::INTBREAK);
        }
        if new & (1 << 8) != 0 {
            status.insert(StatusFlags::INTBREAK);
        }
        if new & (1 << 9) != 0 {
            status.remove(StatusFlags::SIG0);
        }
        if new & (1 << 10) != 0 {
            status.insert(StatusFlags::SIG0);
        }
        if new & (1 << 11) != 0 {
            status.remove(StatusFlags::SIG1);
        }
        if new & (1 << 12) != 0 {
            status.insert(StatusFlags::SIG1);
        }
        if new & (1 << 13) != 0 {
            status.remove(StatusFlags::SIG2);
        }
        if new & (1 << 14) != 0 {
            status.insert(StatusFlags::SIG2);
        }
        if new & (1 << 15) != 0 {
            status.remove(StatusFlags::SIG3);
        }
        if new & (1 << 16) != 0 {
            status.insert(StatusFlags::SIG3);
        }
        if new & (1 << 17) != 0 {
            status.remove(StatusFlags::SIG4);
        }
        if new & (1 << 18) != 0 {
            status.insert(StatusFlags::SIG4);
        }
        if new & (1 << 19) != 0 {
            status.remove(StatusFlags::SIG5);
        }
        if new & (1 << 20) != 0 {
            status.insert(StatusFlags::SIG5);
        }
        if new & (1 << 21) != 0 {
            status.remove(StatusFlags::SIG6);
        }
        if new & (1 << 22) != 0 {
            status.insert(StatusFlags::SIG6);
        }
        if new & (1 << 23) != 0 {
            status.remove(StatusFlags::SIG7);
        }
        if new & (1 << 24) != 0 {
            status.insert(StatusFlags::SIG7);
        }

        info!(self.logger, "write status reg"; "val" => new.hex(), "status" => ?status);

        self.set_status(status)
    }

    // Change the RSP status. Return an Option that says whether the the halt
    // line of the RSP must be changed, and how.
    #[must_use]
    pub(crate) fn set_status(&mut self, status: StatusFlags) -> Option<bool> {
        let changed = self.get_status() ^ status;
        self.reg_status.set(status.bits());

        // HALT status changed, propagate effects to CPU
        if changed.contains(StatusFlags::HALT) {
            if status.contains(StatusFlags::HALT) {
                if status.contains(StatusFlags::INTBREAK) {
                    self.mi.borrow_mut().set_irq_line(IrqMask::SP, true);
                }
                return Some(true);
            } else {
                // Restore execution. RESET is *NOT* performed:
                // execution continues from the point where it was halted
                // before (verified on real hardware).
                info!(self.logger, "RSP started");
                return Some(false);
            }
        }
        None
    }

    fn cb_read_reg_dma_full(&self, _old: u32) -> u32 {
        self.get_status().contains(StatusFlags::DMAFULL) as u32
    }
    fn cb_read_reg_dma_busy(&self, _old: u32) -> u32 {
        self.get_status().contains(StatusFlags::DMABUSY) as u32
    }

    fn cb_read_reg_semaphore(&self, old: u32) -> u32 {
        if old == 0 {
            // Semaphore is acquired when read as 0.
            // self.reg_semaphore.set(1);
        }
        old
    }

    fn dma_xfer(
        &self,
        mut src: u32,
        mut dst: u32,
        width: usize,
        count: usize,
        skip_src: usize,
        skip_dst: usize,
    ) {
        let mut bus = self.main_bus.borrow_mut();
        for _ in 0..count {
            let src_hwio = bus.fetch_read::<u8>(src);
            let mut dst_hwio = bus.fetch_write::<u8>(dst);
            let src_mem = src_hwio.mem().unwrap();
            let dst_mem = dst_hwio.mem().unwrap();
            dst_mem[0..width].copy_from_slice(&src_mem[0..width]);

            src += (width + skip_src) as u32;
            dst += (width + skip_dst) as u32;
        }
    }

    fn cb_write_reg_dma_rd_len(&self, _old: u32, val: u32) {
        let width = (val & 0xFFF) as usize + 1;
        let count = ((val >> 12) & 0xFF) as usize + 1;
        let skip = ((val >> 20) & 0xFFF) as usize;

        // Addresses are treated as 64-bit aligned.
        let src = self.reg_dma_rdram_addr.get() & !0x7;
        let dst = self.reg_dma_rsp_addr.get() & !0x7;

        info!(self.logger, "DMA xfer: RDRAM -> RSP"; o!(
            "rdram" => src.hex(),
            "rsp" =>  dst.hex(),
            "width" => width,
            "count" => count,
            "skip" => skip,
        ));

        self.dma_xfer(src, dst + 0x0400_0000, width, count, skip, 0);
    }

    fn cb_write_reg_dma_wr_len(&self, _old: u32, val: u32) {
        let width = (val & 0xFFF) as usize + 1;
        let count = ((val >> 12) & 0xFF) as usize + 1;
        let skip = ((val >> 20) & 0xFFF) as usize;

        info!(self.logger, "DMA xfer: RSP -> RDRAM"; o!(
            "rsp" =>  self.reg_dma_rsp_addr.get().hex(),
            "rdram" => self.reg_dma_rdram_addr.get().hex(),
            "width" => width,
            "count" => count,
            "skip" => skip,
        ));

        self.dma_xfer(
            self.reg_dma_rsp_addr.get() + 0x0400_0000,
            self.reg_dma_rdram_addr.get(),
            width,
            count,
            0,
            skip,
        );
    }

    fn cb_write_reg_rsp_pc(&self, _old: u32, val: u32) {
        info!(self.logger, "RSP set PC"; o!("pc" => val.hex()));
        self.core_cpu
            .as_ref()
            .unwrap()
            .borrow_mut()
            .ctx_mut()
            .set_pc(val as u64);
    }

    fn cb_read_reg_rsp_pc(&self, _old: u32) -> u32 {
        self.core_cpu.as_ref().unwrap().borrow().ctx().get_pc() as u32 & 0xFFF
    }
}
