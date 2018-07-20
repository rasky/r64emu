extern crate emu;
use super::bl::Blender;
use super::cc::Combiner;
use super::MultiColor;
use emu::gfx::{Color, Rgba8888};

pub struct PixelPipeline {
    cc: Combiner,
    bl: Blender,
}

impl PixelPipeline {
    pub fn new() -> PixelPipeline {
        PixelPipeline {
            cc: Combiner::new(),
            bl: Blender::new(),
        }
    }

    #[inline(always)]
    pub fn calc_pixels(&mut self, shade: MultiColor, fb: MultiColor) -> MultiColor {
        self.cc.set_tex0(shade);
        let combined = self.cc.combine_1cycle(shade);
        let blended = self.bl.blend_1cycle(combined, shade, fb);
        return blended;
    }

    pub fn set_combine_mode(&mut self, mode: u64) {
        self.cc.set_mode(mode);
    }
    pub fn set_prim_color(&mut self, c: Color<Rgba8888>) {
        self.cc.set_prim(c);
    }
    pub fn set_env_color(&mut self, c: Color<Rgba8888>) {
        self.cc.set_env(c);
    }
    pub fn set_blend_color(&mut self, c: Color<Rgba8888>) {
        self.bl.set_blend_color(c);
    }
    pub fn set_other_modes(&mut self, modes: u64) {
        self.bl.set_other_modes(modes);
    }

    pub fn fmt_combiner(&self) -> String {
        self.cc.fmt_1cycle()
    }
    pub fn fmt_blender(&self) -> String {
        self.bl.fmt_1cycle()
    }
}
