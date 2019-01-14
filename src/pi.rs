use super::mi::{IrqMask, Mi};
use super::n64::{JOY_NAMES, R4300};
use super::si::Si;
use crate::errors::*;
use bitfield::Bit;
use byteorder::{BigEndian, ByteOrder};
use emu::bus::be::{Device, Mem, MemFlags, Reg32};
use emu::dbg;
use emu::input::{InputManager, InputValue};
use emu::int::Numerics;
use emu::state::Field;
use emu::sync;
use emu_derive::DeviceBE;
use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::Path;
use std::result;

#[derive(DeviceBE)]
pub struct Pi {
    #[mem(bank = 1, offset = 0x0, vsize = 0x7C0)]
    rom: Mem,

    #[mem(bank = 1, offset = 0x7C0, size = 0x40)]
    ram: Mem,

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
    //     [2] error                     [1] clear intr
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
    cycles: Field<i64>,
    pub(crate) input: InputManager,
}

impl Pi {
    pub fn new(logger: slog::Logger, pifrom: &Path, input: InputManager) -> Result<Box<Pi>> {
        let mut contents = vec![];
        File::open(pifrom)?.read_to_end(&mut contents)?;

        Ok(Box::new(Pi {
            logger,
            rom: Mem::from_buffer("pif_rom", contents, MemFlags::READACCESS),
            ram: Mem::default(),
            cycles: Field::new("Pi::cycles", 0),
            input: input,
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
        }))
    }

    fn cb_write_dma_status(&mut self, old: u32, new: u32) {
        self.dma_status.set(old); // write bits are not related to read bits
        info!(self.logger, "write dma status"; o!("val" => format!("{:x}", new)));
        Mi::get_mut().set_irq_line(IrqMask::PI, false);
    }

    fn cb_write_dma_wr_len(&mut self, _old: u32, len: u32) {
        let mut raddr = self.dma_rom_addr.get();
        let mut waddr = self.dma_ram_addr.get();
        info!(self.logger, "DMA xfer"; o!(
            "src(rom)" => raddr.hex(),
            "dst(ram)" => waddr.hex(),
            "len" => len+1));

        let bus = &mut R4300::get_mut().bus;
        let mut i = 0;
        while i < len + 1 {
            let data = bus.read::<u32>(raddr);
            bus.write::<u32>(waddr, data);
            raddr = raddr + 4;
            waddr = waddr + 4;
            i += 4;
        }
        self.dma_rom_addr.set(raddr);
        self.dma_ram_addr.set(waddr);
        Mi::get_mut().set_irq_line(IrqMask::PI, true);
    }

    fn cb_write_dma_rd_len(&mut self, _old: u32, val: u32) {
        let mut raddr = self.dma_ram_addr.get();
        let mut waddr = self.dma_rom_addr.get();
        info!(self.logger, "DMA xfer"; o!(
            "src(ram)" => raddr.hex(),
            "dst(rom)" => waddr.hex(),
            "len" => val+1));

        let bus = &mut R4300::get_mut().bus;
        let mut i = 0;
        while i < val + 1 {
            let v = bus.read::<u32>(raddr);
            info!(self.logger, "DMA DATA"; "data" => v.hex());

            raddr = raddr + 4;
            waddr = waddr + 4;
            i += 4;
        }
        Mi::get_mut().set_irq_line(IrqMask::PI, true);

        unimplemented!();
    }

    pub fn begin_frame(&mut self) {
        self.input.begin_frame();
    }
    pub fn end_frame(&mut self) {
        self.input.end_frame();
    }

    fn joybus_cmd(
        &mut self,
        ch: usize,
        cmd: Range<usize>,
        out: Range<usize>,
    ) -> result::Result<(), &'static str> {
        if cmd.len() == 0 {
            return Err("joybus: 0-len command");
        }

        match self.ram[cmd.start] {
            0 => {
                // Read controller status
                if ch == 0 {
                    self.ram[out.start + 0] = 0x05;
                    self.ram[out.start + 1] = 0x00;
                    self.ram[out.start + 2] = 0x02;
                }
            }
            1 => {
                // Read input data
                if ch < 4 {
                    let mut value: u32 = 0;
                    self.input
                        .device(JOY_NAMES[ch])
                        .unwrap()
                        .visit(|i| match i.value() {
                            InputValue::Digital(val) => {
                                if val {
                                    value.set_bit(i.custom_id(), true);
                                }
                            }
                            InputValue::Analog(val) => {
                                value |= ((val >> 8) as u8 as u32) << i.custom_id()
                            }
                            _ => unreachable!(),
                        });

                    // S+Left+Right => Reset.
                    if value.bit(21) && value.bit(20) && value.bit(18) {
                        value.set_bit(23, true);
                    }

                    BigEndian::write_u32(&mut self.ram[out.start..], value);
                }
            }
            _ => {
                return Err("invalid command");
            }
        };

        return Ok(());
    }

    fn joybus_exec(&mut self) -> result::Result<(), &'static str> {
        let mut ch = 0;
        let mut idx = 0;
        while idx < 0x3F {
            let t = self.ram[idx];
            idx += 1;
            if t == 0xFE {
                // Special marker: end of joybus
                return Ok(());
            }
            if t < 0x80 {
                let r = *self.ram.get(idx).ok_or("joybus: premature end of RAM")?;
                idx += 1;

                if ch >= 1 {
                    self.ram[idx - 1] |= 0x80;
                }

                let mid = idx + t as usize;
                let end = mid + r as usize;
                self.joybus_cmd(ch, idx..mid, mid..end)?;
                idx = end;
                ch += 1;
            }
        }
        return Err("joybus: no PIFRAM marker found");
    }
}

impl sync::Subsystem for Pi {
    fn name(&self) -> &str {
        "Pi"
    }

    fn run(&mut self, target_cycles: i64, _tracer: &dbg::Tracer) -> dbg::Result<()> {
        // FIXME: we have no timing info at the moment. Let's just do everything
        // we can when we are called.
        *self.cycles = target_cycles;

        let status = self.ram[0x3F];
        if status & 0x20 != 0 {
            info!(self.logger, "unlock boot");
            self.ram[0x3F] |= 0x80;
            self.ram[0x3F] &= !0x20;
        }

        if status & 0x01 != 0 {
            info!(self.logger, "joybus triggered");
            self.joybus_exec();
            self.ram[0x3F] &= !1;

            let mut mem = self.ram.iter();
            for i in 0..8 {
                println!(
                    "JOY: {:03x}: {:02x} {:02x} {:02x} {:02x} -- {:02x} {:02x} {:02x} {:02x}",
                    i * 8,
                    mem.next().unwrap(),
                    mem.next().unwrap(),
                    mem.next().unwrap(),
                    mem.next().unwrap(),
                    mem.next().unwrap(),
                    mem.next().unwrap(),
                    mem.next().unwrap(),
                    mem.next().unwrap()
                );
            }

            Si::get_mut().set_busy(false);
        }

        Ok(())
    }

    fn step(&mut self, _tracer: &dbg::Tracer) -> dbg::Result<()> {
        panic!("Pi::step() should never be called");
    }
    fn cycles(&self) -> i64 {
        *self.cycles
    }
    fn pc(&self) -> Option<u64> {
        None // No program counter
    }
}
