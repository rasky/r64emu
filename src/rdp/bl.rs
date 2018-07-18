// Blender

// TODO:
//   * alpha compare

extern crate bit_field;

use self::bit_field::BitField;
use super::{Color, MColor, MultiColor};
use std::ptr;

struct BlenderCycle {
    p: *const MultiColor,
    m: *const MultiColor,
    a: *const MultiColor,
    b: *const MultiColor,
}

impl BlenderCycle {
    fn fetch(&self) -> (MultiColor, MultiColor, MultiColor, MultiColor) {
        unsafe { (*self.p, *self.m, *self.a, *self.b) }
    }
}

impl Default for BlenderCycle {
    fn default() -> Self {
        BlenderCycle {
            p: ptr::null(),
            m: ptr::null(),
            a: ptr::null(),
            b: ptr::null(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Blender {
    combined: MultiColor,
    shade: MultiColor,
    inv_muxa: MultiColor,
    partial_blended: MultiColor,
    framebuffer: MultiColor,
    reg_blend: MultiColor,
    reg_fog: MultiColor,

    zero: MultiColor, // 0x00
    ff: MultiColor,   // 0xFF

    cycles: [BlenderCycle; 2],
}

impl Blender {
    pub(crate) fn new() -> Blender {
        Blender {
            ff: MultiColor::splat(0xff),
            ..Default::default()
        }
    }

    #[inline(always)]
    pub(crate) fn blend_1cycle(
        &mut self,
        combined: MultiColor,
        shade: MultiColor,
        fb: MultiColor,
    ) -> MultiColor {
        self.combined = combined;
        self.shade = shade;
        self.framebuffer = fb;

        let (p, m, a, b) = self.cycles[0].fetch();
        let a = a.replicate_alpha() >> 3;
        let b = (b.replicate_alpha() >> 3) + MultiColor::splat(1);

        (p * a + m * b) / (a + b)
    }

    pub(crate) unsafe fn setup_cycle_pm(&self, cyc: usize, p_or_m: u32) -> *const MultiColor {
        match p_or_m {
            0 => if cyc == 0 {
                &self.combined
            } else {
                &self.partial_blended
            },
            1 => &self.framebuffer,
            2 => &self.reg_blend,
            3 => &self.reg_fog,
            _ => unreachable!(),
        }
    }

    unsafe fn setup_cycle(&self, cyc: usize, (p, m, a, b): (u32, u32, u32, u32)) -> BlenderCycle {
        BlenderCycle {
            p: self.setup_cycle_pm(cyc, p),
            m: self.setup_cycle_pm(cyc, m),
            a: match a {
                0 => &self.combined,
                1 => &self.reg_fog,
                2 => &self.shade,
                3 => &self.zero,
                _ => unreachable!(),
            },
            b: match b {
                0 => &self.inv_muxa,
                1 => &self.framebuffer,
                2 => &self.ff,
                3 => &self.zero,
                _ => unreachable!(),
            },
        }
    }

    pub(crate) fn set_other_modes(&mut self, modes: u64) {
        let p = modes.get_bits(30..32) as u32;
        let m = modes.get_bits(26..28) as u32;
        let a = modes.get_bits(22..24) as u32;
        let b = modes.get_bits(18..20) as u32;
        self.cycles[0] = unsafe { self.setup_cycle(0, (p, m, a, b)) };

        let p = modes.get_bits(28..30) as u32;
        let m = modes.get_bits(24..26) as u32;
        let a = modes.get_bits(20..22) as u32;
        let b = modes.get_bits(16..18) as u32;
        self.cycles[1] = unsafe { self.setup_cycle(1, (p, m, a, b)) };
    }

    pub(crate) fn set_fog_color(&mut self, c: Color) {
        self.reg_fog = MultiColor::from_color(c);
    }
    pub(crate) fn set_blend_color(&mut self, c: Color) {
        self.reg_blend = MultiColor::from_color(c);
    }
}
