extern crate byteorder;
extern crate emu;
extern crate slog;
use emu::bus::be::{Bus, Reg32};
use emu::gfx::*;
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

    pub fn draw_frame(&self, screen: &mut GfxBufferMutLE<Rgb888>) {
        let bpp = self.status.get() & 3;

        // display disable -> clear screen
        if bpp == 0 || bpp == 1 {
            let black = Color::<Rgb888>::new_clamped(0, 0, 0, 0);
            for y in 0..480 {
                let mut line = screen.line(y);
                for x in 0..640 {
                    line.set(x, black);
                }
            }
            return;
        }

        info!(self.logger, "draw frame"; o!("origin" => self.origin.get().hex()));
        let memio = self.bus.borrow().fetch_read::<u8>(self.origin.get());
        let src = memio.mem().unwrap();

        match self.width.get() {
            640 => {
                let src = GfxBufferLE::<Rgb888>::new(src, 640, 480, 640 * 4).unwrap();
                for y in 0..480 {
                    let mut dst = screen.line(y);
                    let src = src.line(y);
                    for x in 0..640 {
                        dst.set(x, src.get(x));
                    }
                }
            }

            320 => {
                match bpp {
                    // 32-bit
                    3 => {
                        let src = GfxBufferLE::<Rgb888>::new(src, 320, 240, 320 * 4).unwrap();
                        for y in 0..240 {
                            let (mut dst1, mut dst2) = screen.lines(y * 2, y * 2 + 1);
                            let src = src.line(y);
                            for x in 0..320 {
                                let px = src.get(x);
                                dst1.set(x * 2, px);
                                dst1.set(x * 2 + 1, px);
                                dst2.set(x * 2, px);
                                dst2.set(x * 2 + 1, px);
                            }
                        }
                    }
                    // 16-bit
                    2 => {
                        let src = GfxBufferLE::<Rgb565>::new(src, 320, 240, 320 * 2).unwrap();
                        for y in 0..240 {
                            let (mut dst1, mut dst2) = screen.lines(y * 2, y * 2 + 1);
                            let src = src.line(y);
                            for x in 0..320 {
                                let px = src.get(x).cconv();
                                dst1.set(x * 2, px);
                                dst1.set(x * 2 + 1, px);
                                dst2.set(x * 2, px);
                                dst2.set(x * 2 + 1, px);
                            }
                        }
                    }
                    _ => unimplemented!(),
                }
            }

            _ => {
                error!(self.logger, "unsupported screen width"; o!("width" => self.width.get()));
            }
        }
    }
}
