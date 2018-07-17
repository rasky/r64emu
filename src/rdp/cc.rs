// Color Combiner

extern crate bit_field;
extern crate emu;
extern crate enum_map;

use self::bit_field::BitField;
use self::emu::fp::formats::*;
use self::emu::fp::Q;
use self::enum_map::{Enum, EnumMap};

type Component = Q<U8F8>;
type Color = [Component; 4];

#[derive(Debug, Enum, Copy, Clone)]
enum Input {
    CombinedR,
    CombinedG,
    CombinedB,
    CombinedA,

    Texel0R,
    Texel0G,
    Texel0B,
    Texel0A,

    Texel1R,
    Texel1G,
    Texel1B,
    Texel1A,

    PrimR,
    PrimG,
    PrimB,
    PrimA,

    ShadeR,
    ShadeG,
    ShadeB,
    ShadeA,

    EnvR,
    EnvG,
    EnvB,
    EnvA,

    KeyCenter,
    KeyScale,

    LodFraction,
    PrimLodFraction,
    Noise,
    ConvK4,
    ConvK5,

    One,
    Zero,
}

struct CombinerMode(u64);
struct CombinerCycle {
    suba: Input,
    subb: Input,
    mul: Input,
    add: Input,
}

struct InputTable {
    suba: [Input; 16],
    subb: [Input; 16],
    mul: [Input; 32],
    add: [Input; 16],
}

impl InputTable {
    fn as_combiner_cycle(&self, (suba, subb, mul, add): (u32, u32, u32, u32)) -> CombinerCycle {
        CombinerCycle {
            suba: self.suba[suba as usize],
            subb: self.subb[subb as usize],
            mul: self.mul[mul as usize],
            add: self.add[add as usize],
        }
    }
}

impl CombinerMode {
    #[inline]
    fn cyc0_rgb(&self) -> (u32, u32, u32, u32) {
        (
            self.0.get_bits(52..56) as u32,
            self.0.get_bits(28..32) as u32,
            self.0.get_bits(47..52) as u32,
            self.0.get_bits(15..18) as u32,
        )
    }

    #[inline]
    fn cyc0_alpha(&self) -> (u32, u32, u32, u32) {
        (
            self.0.get_bits(44..47) as u32,
            self.0.get_bits(12..15) as u32,
            self.0.get_bits(41..44) as u32,
            self.0.get_bits(9..12) as u32,
        )
    }

    #[inline]
    fn cyc1_rgb(&self) -> (u32, u32, u32, u32) {
        (
            self.0.get_bits(37..41) as u32,
            self.0.get_bits(24..28) as u32,
            self.0.get_bits(32..37) as u32,
            self.0.get_bits(6..9) as u32,
        )
    }

    #[inline]
    fn cyc1_alpha(&self) -> (u32, u32, u32, u32) {
        (
            self.0.get_bits(21..24) as u32,
            self.0.get_bits(3..6) as u32,
            self.0.get_bits(18..21) as u32,
            self.0.get_bits(0..3) as u32,
        )
    }
}

pub struct Combiner {
    inputs: EnumMap<Input, Component>,
    tables: [InputTable; 4],

    cyc0: [CombinerCycle; 4], // cyc0[RGBA][SUB/mul/add]
    cyc1: [CombinerCycle; 4], // cyc1[RGBA][SUB/mul/add]
}

impl Combiner {
    fn fill_input_tables(&mut self) {
        for tidx in 0..4 {
            let t = &mut self.tables[tidx];
            for idx in 0..=5 {
                let input = Enum::<Input>::from_usize((idx * 4 + tidx) as usize);
                t.suba[idx] = input;
                t.subb[idx] = input;
                t.mul[idx] = input;
                t.add[idx] = input;
            }
            for idx in 8..16 {
                t.suba[idx] = Input::Zero;
                t.subb[idx] = Input::Zero;
                t.mul[idx] = Input::Zero;
                t.add[idx] = Input::Zero;
            }
            for idx in 16..32 {
                t.mul[idx] = Input::Zero;
            }
        }

        for tidx in 0..3 {
            let t = &mut self.tables[tidx];

            t.suba[6] = Input::One;
            t.suba[7] = Input::Noise;

            t.subb[6] = Input::KeyCenter;
            t.subb[7] = Input::ConvK4;

            t.mul[6] = Input::KeyScale;
            t.mul[7] = Input::CombinedA;
            t.mul[8] = Input::Texel0A;
            t.mul[9] = Input::Texel1A;
            t.mul[10] = Input::PrimA;
            t.mul[11] = Input::ShadeA;
            t.mul[12] = Input::EnvA;
            t.mul[13] = Input::LodFraction;
            t.mul[14] = Input::PrimLodFraction;
            t.mul[15] = Input::ConvK5;

            t.add[6] = Input::One;
            t.add[7] = Input::Zero;
        }

        let t = &mut self.tables[3];

        t.suba[6] = Input::One;
        t.suba[7] = Input::Zero;

        t.subb[6] = Input::One;
        t.subb[7] = Input::Zero;

        t.mul[0] = Input::LodFraction;
        t.mul[6] = Input::PrimLodFraction;
        t.mul[7] = Input::Zero;

        t.add[6] = Input::One;
        t.add[7] = Input::Zero;
    }

    #[inline(always)]
    fn combine_cyc0_rgb(&self, idx: usize) -> Component {
        let suba = self.inputs[self.cyc0[idx].suba];
        let subb = self.inputs[self.cyc0[idx].subb];
        let mul = self.inputs[self.cyc0[idx].mul];
        let add = self.inputs[self.cyc0[idx].add];
        (suba - subb) * mul + add
    }

    #[inline(always)]
    fn combine_cyc1_rgb(&self, idx: usize) -> Component {
        let suba = self.inputs[self.cyc1[idx].suba];
        let subb = self.inputs[self.cyc1[idx].subb];
        let mul = self.inputs[self.cyc1[idx].mul];
        let add = self.inputs[self.cyc1[idx].add];
        (suba - subb) * mul + add
    }

    pub fn set_mode(&mut self, mode: u64) {
        let mode = CombinerMode(mode);

        let cyc0_rgb = mode.cyc0_rgb();
        let cyc1_rgb = mode.cyc1_rgb();
        for i in 0..3 {
            self.cyc0[i] = self.tables[i].as_combiner_cycle(cyc0_rgb);
            self.cyc1[i] = self.tables[i].as_combiner_cycle(cyc1_rgb);
        }

        let cyc0_alpha = mode.cyc0_alpha();
        let cyc1_alpha = mode.cyc1_alpha();
        self.cyc0[3] = self.tables[3].as_combiner_cycle(cyc0_alpha);
        self.cyc1[3] = self.tables[3].as_combiner_cycle(cyc1_alpha);
    }

    pub fn set_tex0(&mut self, c: Color) {
        self.inputs[Input::Texel0R] = c[0];
        self.inputs[Input::Texel0G] = c[1];
        self.inputs[Input::Texel0B] = c[2];
        self.inputs[Input::Texel0A] = c[3];
    }

    pub fn set_tex1(&mut self, c: Color) {
        self.inputs[Input::Texel1R] = c[0];
        self.inputs[Input::Texel1G] = c[1];
        self.inputs[Input::Texel1B] = c[2];
        self.inputs[Input::Texel1A] = c[3];
    }

    pub fn set_prim(&mut self, c: Color) {
        self.inputs[Input::PrimR] = c[0];
        self.inputs[Input::PrimG] = c[1];
        self.inputs[Input::PrimB] = c[2];
        self.inputs[Input::PrimA] = c[3];
    }

    pub fn set_shade(&mut self, c: Color) {
        self.inputs[Input::ShadeR] = c[0];
        self.inputs[Input::ShadeG] = c[1];
        self.inputs[Input::ShadeB] = c[2];
        self.inputs[Input::ShadeA] = c[3];
    }

    pub fn set_env(&mut self, c: Color) {
        self.inputs[Input::EnvR] = c[0];
        self.inputs[Input::EnvG] = c[1];
        self.inputs[Input::EnvB] = c[2];
        self.inputs[Input::EnvA] = c[3];
    }
}
