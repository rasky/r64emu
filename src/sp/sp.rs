extern crate emu;
extern crate slog;

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
        const HALT =            0b00000001;
        const BROKE =           0b00000010;
        const DMABUSY =         0b00000100;
        const DMAFULL =         0b00001000;
        const IOFULL =         0b000010000;
        const SINGLESTEP =    0b0000100000;
        const INTBREAK =     0b00001000000;
        const SIG0 =        0b000010000000;
        const SIG1 =       0b0000100000000;
        const SIG2 =      0b00001000000000;
        const SIG3 =     0b000010000000000;
        const SIG4 =    0b0000100000000000;
        const SIG5 =   0b00001000000000000;
        const SIG6 =  0b000010000000000000;
        const SIG7 = 0b0000100000000000000;
    }
}

#[derive(DeviceBE)]
pub struct Sp {
    pub core_cpu: Rc<RefCell<Box<mips64::Cpu>>>,
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
}

impl Sp {
    pub fn new(logger: slog::Logger, main_bus: Rc<RefCell<Box<Bus>>>) -> Result<DevPtr<Sp>> {
        // Create the RSP internal MIPS CPU and its associated bus
        let bus = Rc::new(RefCell::new(Bus::new(logger.new(o!()))));
        let cpu = Rc::new(RefCell::new(Box::new(mips64::Cpu::new(
            "RSP",
            logger.new(o!()),
            bus.clone(),
        ))));

        let sp = DevPtr::new(Sp {
            logger,
            main_bus,
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

            core_bus: bus,
            core_cpu: cpu,
        });

        {
            // Configure CPU:
            //   COP0: special unit to access RSP registers (no exception support)
            //   COP2: vector unit
            let spb = sp.borrow();
            let mut cpu = spb.core_cpu.borrow_mut();
            cpu.set_cop0(SpCop0::new(&sp, spb.logger.new(o!()))?);
            cpu.set_cop2(SpCop2::new(&sp, spb.logger.new(o!()))?);
            cpu.bus_write_mask = 0xFFF;
            cpu.bus_read_mask = 0xFFF;
            cpu.bus_fetch_mask = 0xFFF;
            cpu.bus_fetch_fixed = 0x1000;

            let ctx = cpu.ctx_mut();
            ctx.set_halt_line(true);
            ctx.set_pc(0);
        }

        {
            // Configure RSP internal core bus
            let spb = sp.borrow();
            let mut bus = spb.core_bus.borrow_mut();
            bus.map_device(0x0000_0000, &sp, 0)?;
        }

        Ok(sp)
    }

    pub(crate) fn get_status(&self) -> StatusFlags {
        StatusFlags::from_bits(self.reg_status.get()).unwrap()
    }

    fn cb_write_reg_status(&mut self, old: u32, new: u32) {
        self.reg_status.set(old); // restore previous value, as write bits are completely different

        let mut status = self.get_status();
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
            error!(self.logger, "clear RSP Interrupt not implemented");
        }
        if new & (1 << 4) != 0 {
            error!(self.logger, "set RSP Interrupt not implemented");
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
        let mut cpu = self.core_cpu.borrow_mut();
        self.set_status(status, cpu.ctx_mut());
    }

    pub(crate) fn set_status(&self, status: StatusFlags, ctx: &mut mips64::CpuContext) {
        let changed = self.get_status() ^ status;
        self.reg_status.set(status.bits());

        // HALT status changed, propagate effects to CPU
        if changed.contains(StatusFlags::HALT) {
            if status.contains(StatusFlags::HALT) {
                ctx.set_halt_line(true);
                if status.contains(StatusFlags::INTBREAK) {
                    // FIXME: generate interrupt on break
                }
            } else {
                // Restore execution. RESET is *NOT* performed:
                // execution continues from the point where it was halted
                // before (verified on real hardware).
                ctx.set_halt_line(false);
                info!(self.logger, "RSP started");
            }
        }
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
            self.reg_semaphore.set(1);
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
        let bus = self.main_bus.borrow();
        for _ in 0..count {
            let src_hwio = bus.fetch_read::<u8>(src);
            let dst_hwio = bus.fetch_write::<u8>(dst);
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
        self.core_cpu.borrow_mut().ctx_mut().set_pc(val as u64);
    }

    fn cb_read_reg_rsp_pc(&self, _old: u32) -> u32 {
        self.core_cpu.borrow().ctx().get_pc() as u32 & 0xFFF
    }
}
