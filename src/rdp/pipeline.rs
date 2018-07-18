use super::bl::Blender;
use super::cc::Combiner;
use super::{Color, MultiColor};

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

    pub fn calc_pixels(&mut self, shade: MultiColor, fb: MultiColor) -> MultiColor {
        let combined = self.cc.combine_1cycle(shade);
        let blended = self.bl.blend_1cycle(shade, combined, fb);
        return blended;
    }

    pub fn set_prim_color(&mut self, c: Color) {
        self.cc.set_prim(c);
    }
    pub fn set_env_color(&mut self, c: Color) {
        self.cc.set_env(c);
    }
    pub fn set_other_modes(&mut self, modes: u64) {
        self.bl.set_other_modes(modes);
    }
}
