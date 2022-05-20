use emu::bus::be::{Device, Reg32};
use emu::gfx::*;
use emu::int::Numerics;
use emu_derive::DeviceBE;

use super::mi::{IrqMask, Mi};
use super::r4300::R4300;

use slog;

#[derive(DeviceBE)]
pub struct Vi {
    // [1:0] type[1:0] (pixel size)
    //     0: blank (no data, no sync)
    //     1: reserved
    //     2: 5/5/5/3 ("16" bit)
    //     3: 8/8/8/8 (32 bit)
    // [2] gamma_dither_enable (normally on, unless "special effect")
    // [3] gamma_enable (normally on, unless MPEG/JPEG)
    // [4] divot_enable (normally on if antialiased,
    //     unless decal lines)
    // [5] reserved - always off
    // [6] serrate (always on if interlaced, off if not)
    // [7] reserved - diagnostics only
    // [9:8] anti-alias (aa) mode[1:0]
    //     0: aa & resamp (always fetch extra lines)
    //     1: aa & resamp (fetch extra lines if needed)
    //     2: resamp only (treat as all fully covered)
    //     3: neither (replicate pixels, no interpolate)
    // [11] reserved - diagnostics only
    // [15:12] reserved
    #[reg(offset = 0x00, rwmask = 0xFFFF)]
    status: Reg32,

    // [23:0] frame buffer origin in bytes
    #[reg(offset = 0x04, rwmask = 0xFFFFFF)]
    origin: Reg32,

    // [11:0] frame buffer line width in pixels
    #[reg(offset = 0x08, rwmask = 0xFFF)]
    width: Reg32,

    // [9:0] interrupt when current half-line = V_INTR
    #[reg(offset = 0x0C, rwmask = 0x3FF, wcb)]
    vertical_interrupt: Reg32,

    // [9:0] current half line, sampled once per line (the lsb of
    //       V_CURRENT is constant within a field, and in
    //       interlaced modes gives the field number - which is
    //       constant for non-interlaced modes)
    //       - Writes clears interrupt line
    #[reg(offset = 0x10, rwmask = 0, wcb)]
    current_line: Reg32,

    // [7:0] horizontal sync width in pixels
    // [15:8] color burst width in pixels
    // [19:16] vertical sync width in half lines
    // [29:20] start of color burst in pixels from h-sync
    #[reg(offset = 0x14, rwmask = 0x3FFFFFFF)]
    timing: Reg32,

    // [9:0] number of half-lines per field
    #[reg(offset = 0x18)]
    vertical_sync: Reg32,

    // [11:0] total duration of a line in 1/4 pixel
    // [20:16] a 5-bit leap pattern used for PAL only (h_sync_period)
    #[reg(offset = 0x1C, rwmask = 0x1FFFFF)]
    horizontal_sync: Reg32,

    // [11:0] identical to h_sync_period
    // [27:16] identical to h_sync_period
    #[reg(offset = 0x20, rwmask = 0xFFFFFFF)]
    horizontal_sync_leap: Reg32,

    // [9:0] end of active video in screen pixels
    // [25:16] start of active video in screen pixels
    #[reg(offset = 0x24, rwmask = 0x3FFFFFF)]
    horizontal_video: Reg32,

    // [9:0] end of active video in screen half-lines
    // [25:16] start of active video in screen half-lines
    #[reg(offset = 0x28, rwmask = 0x3FFFFFF)]
    vertical_video: Reg32,

    // [9:0] end of color burst enable in half-lines
    // [25:16] start of color burst enable in half-lines
    #[reg(offset = 0x2C, rwmask = 0x3FFFFFF)]
    vertical_burst: Reg32,

    // [11:0] 1/horizontal scale up factor (2.10 format)
    // [27:16] horizontal subpixel offset (2.10 format)
    #[reg(offset = 0x30, rwmask = 0xFFFFFFF)]
    x_scale: Reg32,

    // [11:0] 1/vertical scale up factor (2.10 format)
    // [27:16] vertical subpixel offset (2.10 format)
    #[reg(offset = 0x34, rwmask = 0xFFFFFFF)]
    y_scale: Reg32,

    logger: slog::Logger,
    framecount: usize,
}

impl Vi {
    pub fn new(logger: slog::Logger) -> Box<Vi> {
        Box::new(Vi {
            status: Reg32::default(),
            origin: Reg32::default(),
            width: Reg32::default(),
            vertical_interrupt: Reg32::default(),
            current_line: Reg32::default(),
            timing: Reg32::default(),
            vertical_sync: Reg32::default(),
            horizontal_sync: Reg32::default(),
            horizontal_sync_leap: Reg32::default(),
            horizontal_video: Reg32::default(),
            vertical_video: Reg32::default(),
            vertical_burst: Reg32::default(),
            x_scale: Reg32::default(),
            y_scale: Reg32::default(),
            logger,
            framecount: 0,
        })
    }

    pub fn set_line(&mut self, y: usize) {
        // FIXME: handle interleaved mode (LSB is fixed within the same field)
        // FIXME: NTSC has 525 lines, what happens to this 9-bit register when line > 512?
        self.current_line.set(y as u32);

        if y as u32 == self.vertical_interrupt.get() {
            Mi::get_mut().set_irq_line(IrqMask::VI, true);
        }
    }

    fn cb_write_current_line(&mut self, _old: u32, _new: u32) {
        info!(self.logger, "ack VI interrupt");
        // Writing the current line register acknowledge the interrupt
        Mi::get_mut().set_irq_line(IrqMask::VI, false);
    }

    fn cb_write_vertical_interrupt(&self, _old: u32, new: u32) {
        info!(self.logger, "change VI interrupt"; "line" => new);
    }

    pub fn begin_frame(&mut self, _screen: &mut GfxBufferMutLE<Rgb888>) {}

    pub fn end_frame(&mut self, screen: &mut GfxBufferMutLE<Rgb888>) {
        self.framecount += 1;

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

        let memio = R4300::get().bus.fetch_read::<u8>(self.origin.get());
        let src = memio.mem().unwrap();

        let wstep = (self.x_scale.get() & 0xFFF) as usize;
        let hstep = (self.y_scale.get() & 0xFFF) as usize / 2;

        let (screen_width, screen_height) = (640, 480);
        let width = ((wstep * (screen_width - 1)) >> 10) + 1;
        let height = ((hstep * (screen_height - 1)) >> 10) + 1;
        let fbwidth = self.width.get() as usize;

        info!(self.logger, "draw frame"; o!(
            "origin" => self.origin.get().hex(), 
            "wstep" => (wstep as u32).hex(),
            "hstep" => (hstep as u32).hex()));

        match bpp {
            // 32-bit
            3 => {
                let src = GfxBufferLE::<Rgb888>::new(src, width, height, fbwidth * 4).unwrap();
                let mut sy = 0;
                for y in 0..screen_height {
                    let mut dst = screen.line(y);
                    let src = src.line(sy >> 10);
                    let mut sx = 0;
                    for x in 0..screen_width {
                        let px = src.get(sx >> 10);
                        dst.set(x, px);
                        sx += wstep;
                    }
                    sy += hstep;
                }
            }
            // 16-bit
            2 => {
                let src = GfxBufferBE::<Xbgr1555>::new(src, width, height, fbwidth * 2).unwrap();
                let mut sy = 0;
                for y in 0..screen_height {
                    let mut dst = screen.line(y);
                    let src = src.line(sy >> 10);
                    let mut sx = 0;
                    for x in 0..screen_width {
                        let px = src.get(sx >> 10).cconv();
                        dst.set(x, px);
                        sx += wstep;
                    }
                    sy += hstep;
                }
            }
            _ => unimplemented!(),
        }
    }
}
