extern crate bit_field;
extern crate emu;
extern crate slog;
use self::bit_field::BitField;
use super::gfx::{draw_rect, draw_rect_slopes};
use emu::bus::be::{Bus, MemIoR, Reg32, RegDeref, RegRef};
use emu::fp::formats::*;
use emu::fp::Q;
use emu::gfx::{GfxBuffer, GfxBufferMut, I4, Point, Rect, Rgb555, Rgb888};
use emu::int::Numerics;
use emu::sync;
use std::cell::RefCell;
use std::rc::Rc;

bitflags! {
    struct StatusFlags: u32 {
        const XBUS_DMA = 1<<0;
        const FREEZE = 1<<1;
        const FLUSH = 1<<2;
        const START_GLK = 1<<3;
        const TMEM_BUSY = 1<<4;
        const PIPE_BUSY = 1<<5;
        const CMD_BUSY = 1<<6;
        const CMDBUF_BUSY = 1<<7;
        const DMA_BUSY = 1<<8;
        const END_VALID = 1<<9;
        const START_VALID = 1<<10;
    }
}

impl RegDeref for StatusFlags {
    type Type = u32;
    fn from(v: u32) -> StatusFlags {
        StatusFlags::from_bits_truncate(v)
    }
    fn to(&self) -> u32 {
        self.bits()
    }
}

#[derive(DeviceBE)]
pub struct Dp {
    #[reg(bank = 0, offset = 0x0, rwmask = 0x00FFFFFF, wcb)]
    cmd_start: Reg32,

    #[reg(bank = 0, offset = 0x4, rwmask = 0x00FFFFFF, wcb)]
    cmd_end: Reg32,

    #[reg(bank = 0, offset = 0x8, readonly)]
    cmd_current: Reg32,

    #[reg(bank = 0, offset = 0xC, rwmask = 0, wcb)]
    cmd_status: Reg32,

    logger: slog::Logger,
    main_bus: Rc<RefCell<Box<Bus>>>,

    fetched_mem: MemIoR<u64>,
    fetched_start_addr: u32,
    fetched_end_addr: u32,
    cycles: i64,
    running: bool,

    gfx: Box<DpGfx>,
}

impl Dp {
    pub fn new(logger: slog::Logger, main_bus: Rc<RefCell<Box<Bus>>>) -> Dp {
        let gfx_logger = logger.new(o!());
        Dp {
            cmd_start: Reg32::default(),
            cmd_end: Reg32::default(),
            cmd_current: Reg32::default(),
            cmd_status: Reg32::default(),
            logger,
            main_bus: main_bus.clone(),
            cycles: 0,
            running: false,
            fetched_mem: MemIoR::default(),
            fetched_start_addr: 0,
            fetched_end_addr: 0,
            gfx: Box::new(DpGfx::new(gfx_logger, main_bus.clone())),
        }
    }

    fn cmd_status_ref(&self) -> RegRef<StatusFlags> {
        self.cmd_status.as_ref::<StatusFlags>()
    }
    fn cmd_current_ref(&self) -> RegRef<u32> {
        self.cmd_current.as_ref::<u32>()
    }

    fn cb_write_cmd_start(&mut self, _old: u32, _new: u32) {
        self.cmd_status
            .as_ref::<StatusFlags>()
            .insert(StatusFlags::START_VALID);
    }

    fn cb_write_cmd_end(&mut self, _old: u32, _new: u32) {
        self.cmd_status
            .as_ref::<StatusFlags>()
            .insert(StatusFlags::END_VALID);
        self.check_start();
    }

    fn cb_write_cmd_status(&mut self, old: u32, new: u32) {
        self.cmd_status.set(old);
        warn!(self.logger, "writing to DP status"; o!("val" => new.hex()));
    }

    fn check_start(&mut self) {
        let mut status = self.cmd_status_ref();
        if !status.contains(StatusFlags::END_VALID) {
            // if there's no pending end ptr, there's nothing to do.
            return;
        }

        // See if the start ptr changed, if so we need to refetch it.
        // Otherwise, continue from current pointer.
        if status.contains(StatusFlags::START_VALID) {
            let start = self.cmd_start.get();
            *self.cmd_current_ref() = start;
            self.fetched_start_addr = start;
            self.fetched_mem = self.main_bus.borrow().fetch_read::<u64>(start);
            if self.fetched_mem.mem().is_none() {
                error!(self.logger, "cmd buffer pointing to non-linear memory"; o!("ptr" => start.hex()));
            }
            status.remove(StatusFlags::START_VALID);
        }

        self.fetched_end_addr = self.cmd_end.get();
        status.remove(StatusFlags::END_VALID);
        self.running = true;
        warn!(
            self.logger,
            "DP start";
            o!("start" => self.fetched_start_addr.hex(), "end" => self.fetched_end_addr.hex())
        );
    }
}

impl sync::Subsystem for Dp {
    fn run(&mut self, until: i64) {
        loop {
            if !self.running {
                self.cycles = until;
                return;
            }

            let mut curr_addr = self.cmd_current_ref();
            for cmd in self
                .fetched_mem
                .iter()
                .unwrap()
                .skip((*curr_addr - self.fetched_start_addr) as usize / 8)
                .take((self.fetched_end_addr - *curr_addr) as usize / 8)
            {
                self.gfx.op(cmd);
                *curr_addr += 8;
                self.cycles += 1;
                if self.cycles >= until {
                    return;
                }
            }

            // Finished the current buffer: stop iteration, but
            // check if there's a new buffer pending
            self.running = false;
            self.check_start();
        }
    }

    fn cycles(&self) -> i64 {
        self.cycles
    }
}

#[derive(Copy, Clone, Debug)]
enum ColorFormat {
    RGBA,
    YUV,
    COLOR_INDEX,
    INTENSITY_ALPHA,
    INTENSITY,
}

impl ColorFormat {
    fn from_bits(bits: usize) -> Option<ColorFormat> {
        match bits {
            0 => Some(ColorFormat::RGBA),
            1 => Some(ColorFormat::YUV),
            2 => Some(ColorFormat::COLOR_INDEX),
            3 => Some(ColorFormat::INTENSITY_ALPHA),
            4 => Some(ColorFormat::INTENSITY),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Default, Debug)]
struct TileDescriptor {
    color_format: ColorFormat,
    bpp: usize,
    pitch: usize,
    tmem_addr: u32,
    palette: usize,
    clamp: [bool; 2],
    mirror: [bool; 2],
    mask: [u32; 2],
    shift: [u32; 2],

    rect: Rect<U30F2>,
}

impl Default for ColorFormat {
    fn default() -> ColorFormat {
        ColorFormat::RGBA
    }
}

#[derive(Copy, Clone, Default, Debug)]
struct ImageFormat {
    color_format: ColorFormat,
    bpp: usize,
    width: usize,
    dram_addr: u32,
}

impl ImageFormat {
    fn pitch(&self) -> usize {
        self.width * self.bpp / 8
    }
}

pub struct DpGfx {
    logger: slog::Logger,
    main_bus: Rc<RefCell<Box<Bus>>>,
    tmem: Box<[u8]>,
    clip: Rect<I30F2>,
    fb: ImageFormat,
    tex: ImageFormat,
    tiles: [TileDescriptor; 8],

    cmdbuf: [u64; 16],
    cmdlen: usize,
}

impl DpGfx {
    fn new(logger: slog::Logger, main_bus: Rc<RefCell<Box<Bus>>>) -> DpGfx {
        let mut tmem = Vec::new();
        tmem.resize(4096, 0);
        DpGfx {
            logger: logger,
            main_bus: main_bus,
            tmem: tmem.into_boxed_slice(),
            clip: Rect::default(),
            fb: ImageFormat::default(),
            tex: ImageFormat::default(),
            tiles: [TileDescriptor::default(); 8],
            cmdbuf: [0u64; 16],
            cmdlen: 0,
        }
    }

    fn parse_color_format(&self, bits: u64) -> ColorFormat {
        ColorFormat::from_bits(bits as usize)
            .or_else(|| {
                error!(self.logger, "invalid color format"; "format" => bits);
                Some(ColorFormat::RGBA)
            })
            .unwrap()
    }

    fn op(&mut self, cmd: u64) {
        self.cmdbuf[self.cmdlen] = cmd;
        self.cmdlen += 1;

        let op = self.cmdbuf[0].get_bits(56..62);
        match op {
            0x2D => {
                // Set Scissor
                self.clip = Rect::from_bits(
                    cmd.get_bits(44..56) as i32,
                    cmd.get_bits(32..44) as i32,
                    cmd.get_bits(12..24) as i32,
                    cmd.get_bits(0..12) as i32,
                );
                info!(self.logger, "DP: Set Scissor"; "clip" => ?self.clip);
                self.cmdlen = 0;
            }
            0x3D | 0x3F => {
                // Set Color/Texture Image
                let format = ImageFormat {
                    color_format: self.parse_color_format(cmd.get_bits(53..56)),
                    bpp: 4 << cmd.get_bits(51..53),
                    width: cmd.get_bits(32..42) as usize + 1,
                    dram_addr: cmd.get_bits(0..26) as u32,
                };

                if op == 0x3F {
                    self.fb = format;
                    info!(self.logger, "DP: Set Color Image"; "format" => ?self.fb);
                } else {
                    self.tex = format;
                    info!(self.logger, "DP: Set Texture Image"; "format" => ?self.tex);
                }
                self.cmdlen = 0;
            }
            0x28 => {
                // Sync Tile
                info!(self.logger, "DP: Sync Tile");
                self.cmdlen = 0;
            }
            0x2F => {
                // Set Other Modes
                warn!(self.logger, "DP: Set Other Modes");
                self.cmdlen = 0;
            }
            0x24 => {
                // Texture rectangle (2 words)
                if self.cmdlen != 2 {
                    return;
                }

                let tile = self.cmdbuf[0].get_bits(24..27) as usize;
                let x1 = self.cmdbuf[0].get_bits(44..56) as u32;
                let y1 = self.cmdbuf[0].get_bits(32..44) as u32;
                let x0 = self.cmdbuf[0].get_bits(12..24) as u32;
                let y0 = self.cmdbuf[0].get_bits(0..12) as u32;
                let mut rect = Rect::<U30F2>::from_bits(x0, y0, x1, y1);

                let s = Q::<I6F10>::from_bits(self.cmdbuf[1].get_bits(48..64) as i16);
                let t = Q::<I6F10>::from_bits(self.cmdbuf[1].get_bits(32..48) as i16);
                let dsdx = Q::<I6F10>::from_bits(self.cmdbuf[1].get_bits(16..32) as i16);
                let dtdy = Q::<I6F10>::from_bits(self.cmdbuf[1].get_bits(0..16) as i16);

                let ptex = Point::new(s, t);
                let slope = Point::new(dsdx, dtdy);
                info!(self.logger, "DP: Textured Rectangle"; "idx" => tile, "screen" => ?rect, "ptex" => ?ptex, "slope" => ?slope);

                let tmem_addr = self.tiles[tile].tmem_addr as usize;
                let tmem_pitch = self.tiles[tile].pitch;
                let tex_rect = self.tiles[tile].rect;
                let tmem = GfxBuffer::<I4>::new(
                    &self.tmem[tmem_addr..],
                    tex_rect.width().floor() as usize + 1,
                    tex_rect.height().floor() as usize + 1,
                    tmem_pitch,
                ).unwrap();

                let fb_writer = self.main_bus.borrow().fetch_write::<u8>(self.fb.dram_addr);
                let mut fb_mem = fb_writer.mem().unwrap();
                let mut fb =
                    GfxBufferMut::<Rgb555>::new(&mut fb_mem, 640, 480, self.fb.pitch()).unwrap();

                // FIXME: draw_rect_slopes() use inclusive rectangles... maybe we need clipping?
                let w = rect.width() - 1;
                let h = rect.height() - 1;
                rect.set_width(w);
                rect.set_height(h);

                draw_rect_slopes(&mut fb, rect, &tmem, ptex.cast::<I22F10>(), slope.cast());

                self.cmdlen = 0;
            }
            0x34 => {
                // Load Tile
                let tile = cmd.get_bits(24..27) as usize;
                let s0 = cmd.get_bits(44..56) as u32;
                let t0 = cmd.get_bits(32..44) as u32;
                let s1 = cmd.get_bits(12..24) as u32;
                let t1 = cmd.get_bits(0..12) as u32;
                let mut rect = Rect::<U30F2>::from_bits(s0, t0, s1, t1);
                info!(self.logger, "DP: Load Tile"; "idx" => tile, "rect" => ?rect);

                // Load_Tile also updates the internal tile rect
                self.tiles[tile].rect = rect;

                let tmem_addr = self.tiles[tile].tmem_addr as usize;
                let tmem_pitch = self.tiles[tile].pitch;
                let tex_reader = self.main_bus.borrow().fetch_read::<u8>(self.tex.dram_addr);
                let tex_mem = tex_reader.mem().unwrap();
                let width = rect.width().floor() as usize + 1;
                let height = rect.height().floor() as usize + 1;

                let copy_width = width.min(self.tex.width); // FIXME: is this correct? See RDPI4Decode
                rect.set_width(Q::from_int(copy_width as u32 - 1));

                let mut tmem = GfxBufferMut::<Rgb555>::new(
                    &mut self.tmem[tmem_addr..],
                    copy_width,
                    height,
                    tmem_pitch,
                ).unwrap();

                let tex = GfxBuffer::<Rgb555>::new(&tex_mem, copy_width, height, self.tex.pitch())
                    .unwrap();

                info!(self.logger, "DP: Load Tile: draw_rect"; "rect" => ?rect);
                draw_rect(
                    &mut tmem,
                    Point::<U30F2>::from_int(0, 0),
                    &tex,
                    rect.cast::<U27F5>(),
                );

                self.cmdlen = 0;
            }
            0x35 => {
                // Set Tile
                let idx = cmd.get_bits(24..27) as usize;
                let color_format = self.parse_color_format(cmd.get_bits(53..56));
                let tile = &mut self.tiles[idx];
                tile.color_format = color_format;
                tile.bpp = 4 << cmd.get_bits(51..53);
                tile.pitch = cmd.get_bits(41..50) as usize * 8;
                tile.tmem_addr = cmd.get_bits(32..41) as u32 * 8;
                tile.palette = cmd.get_bits(20..24) as usize;
                tile.clamp[0] = cmd.get_bit(9);
                tile.clamp[1] = cmd.get_bit(19);
                tile.mirror[0] = cmd.get_bit(8);
                tile.mirror[1] = cmd.get_bit(18);
                tile.mask[0] = (1 << cmd.get_bits(4..8)) - 1;
                tile.mask[1] = (1 << cmd.get_bits(14..18)) - 1;
                tile.shift[0] = cmd.get_bits(0..4) as u32;
                tile.shift[1] = cmd.get_bits(10..14) as u32;
                info!(self.logger, "DP: Set Tile"; "idx" => idx, "format" => ?tile);
                self.cmdlen = 0;
            }
            0x3C => {
                // Set Combine Mode
                warn!(self.logger, "DP: Set Combine Mode");
                self.cmdlen = 0;
            }

            _ => {
                warn!(self.logger, "unimplemented command"; "cmd" => (((cmd>>56)&0x3F) as u8).hex());
                self.cmdlen = 0;
            }
        };
    }
}
