extern crate byteorder;
extern crate emu;
extern crate slog;
use byteorder::{ByteOrder, LittleEndian};
use emu::bus::be::{Bus, Reg32};
use emu::int::Numerics;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(DeviceBE)]
pub struct Vi {
    #[reg(offset = 0x00, rwmask = 0xFFFF)]
    status: Reg32,

    #[reg(offset = 0x04, rwmask = 0xFFFFFF)]
    origin: Reg32,

    #[reg(offset = 0x08, rwmask = 0xFFF)]
    width: Reg32,

    #[reg(offset = 0x10, rwmask = 0, wcb)]
    current_line: Reg32,

    logger: slog::Logger,
    bus: Rc<RefCell<Box<Bus>>>,
}

impl Vi {
    pub fn new(logger: slog::Logger, bus: Rc<RefCell<Box<Bus>>>) -> Vi {
        Vi {
            current_line: Reg32::default(),
            status: Reg32::default(),
            origin: Reg32::default(),
            width: Reg32::default(),
            logger,
            bus,
        }
    }

    pub fn set_line(&self, y: usize) {
        self.current_line.set(y as u32);
    }

    fn cb_write_current_line(&self, _old: u32, new: u32) {
        error!(self.logger, "write VI current line"; o!("val" => new.hex()));
    }

    pub fn draw_frame(&self, screen: &mut [u8], pitch: usize) {
        let bpp = self.status.get() & 3;

        // display disable -> clear screen
        if bpp == 0 || bpp == 1 {
            for y in 0..480 {
                let line = &mut screen[y * pitch..];
                for x in 0..640 {
                    line[x * 4 + 0] = 0;
                    line[x * 4 + 1] = 0;
                    line[x * 4 + 2] = 0;
                    line[x * 4 + 3] = 0;
                }
            }
            return;
        }

        info!(self.logger, "draw frame"; o!("origin" => self.origin.get().hex()));
        let memio = self.bus.borrow().fetch_read::<u8>(self.origin.get());
        let mut src = memio.mem().unwrap();

        match self.width.get() {
            640 => {
                for y in 0..480 {
                    let line = &mut screen[y * pitch..(y + 1) * pitch];
                    line.copy_from_slice(&src[..640 * 4]);
                    src = &src[640 * 4..];
                }
            }

            320 => {
                for y in 0..240 {
                    let (line1, line2) =
                        &mut screen[y * 2 * pitch..(y + 1) * 2 * pitch].split_at_mut(pitch);

                    match bpp {
                        3 => {
                            // 32-bit
                            for x in 0..320 {
                                let mut px = LittleEndian::read_u32(&src[x * 4..x * 4 + 4]);
                                px |= 0xffff_ffff;
                                LittleEndian::write_u32(&mut line1[x * 8..x * 8 + 4], px);
                                LittleEndian::write_u32(&mut line2[x * 8..x * 8 + 4], px);
                                LittleEndian::write_u32(&mut line1[x * 8 + 4..x * 8 + 8], px);
                                LittleEndian::write_u32(&mut line2[x * 8 + 4..x * 8 + 8], px);
                            }
                            src = &src[320 * 4..];
                        }
                        _ => unimplemented!(),
                    };
                }
            }

            _ => {
                error!(self.logger, "unsupported screen width"; o!("width" => self.width.get()));
            }
        }
    }
}
