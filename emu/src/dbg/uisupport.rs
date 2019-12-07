use imgui::sys;
use imgui::*;
use std::fmt;
use std::time::{Duration, Instant};

pub(crate) struct ImGuiListClipper {
    items_count: usize,
    items_height: f32,
}

impl ImGuiListClipper {
    pub(crate) fn new(items_count: usize) -> Self {
        Self {
            items_count: items_count,
            items_height: -1.0,
        }
    }

    pub(crate) fn items_count(mut self, items_count: usize) -> Self {
        self.items_count = items_count;
        self
    }

    pub(crate) fn items_height(mut self, items_height: f32) -> Self {
        self.items_height = items_height;
        self
    }

    pub(crate) fn build<F: FnMut(isize, isize)>(self, mut f: F) {
        let mut clip: sys::ImGuiListClipper = unsafe { ::std::mem::uninitialized() };
        unsafe {
            sys::ImGuiListClipper_Begin(&mut clip, self.items_count as _, self.items_height as _);
        }

        loop {
            let done = unsafe { sys::ImGuiListClipper_Step(&mut clip) };
            if !done {
                break;
            }
            f(clip.DisplayStart as isize, clip.DisplayEnd as isize);
        }

        unsafe {
            sys::ImGuiListClipper_End(&mut clip);
        }
    }
}

pub trait HexableInt: Copy + fmt::Display {
    const HEX_DIGITS: usize;
    fn format(self) -> String;
    fn parse(s: &str) -> Option<Self>;
}

impl HexableInt for u8 {
    const HEX_DIGITS: usize = 2;
    fn format(self) -> String {
        format!("{:02x}", self)
    }
    fn parse(s: &str) -> Option<Self> {
        u8::from_str_radix(s, 16).ok()
    }
}
impl HexableInt for u16 {
    const HEX_DIGITS: usize = 4;
    fn format(self) -> String {
        format!("{:04x}", self)
    }
    fn parse(s: &str) -> Option<Self> {
        u16::from_str_radix(s, 16).ok()
    }
}
impl HexableInt for u32 {
    const HEX_DIGITS: usize = 8;
    fn format(self) -> String {
        format!("{:08x}", self)
    }
    fn parse(s: &str) -> Option<Self> {
        u32::from_str_radix(s, 16).ok()
    }
}
impl HexableInt for u64 {
    const HEX_DIGITS: usize = 16;
    fn format(self) -> String {
        format!("{:016x}", self)
    }
    fn parse(s: &str) -> Option<Self> {
        u64::from_str_radix(s, 16).ok()
    }
}

pub fn imgui_input_hex<T: HexableInt>(
    ui: &Ui<'_>,
    name: &ImStr,
    val: &mut T,
    wait_enter: bool,
) -> bool {
    let mut changed = false;

    let iw = ui.push_item_width(T::HEX_DIGITS as f32 * 7.0 + 8.0);

    let vals = val.format();

    // Create a buffer with the exact capacity needed to store this number.
    // This blocks Imgui from letting the user put more digits than allowed.
    let mut spc = ImString::with_capacity(vals.len());
    spc.push_str(&vals);

    // Draw the input text
    if ui
        .input_text(name, &mut spc)
        .chars_hexadecimal(true)
        .enter_returns_true(wait_enter)
        .auto_select_all(true)
        .build()
    {
        if let Some(v) = T::parse(spc.as_ref()) {
            *val = v;
            changed = true;
        }
    }

    iw.pop(&ui);

    changed
}

fn interp4(a: [f32; 4], b: [f32; 4], d: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * d,
        a[1] + (b[1] - a[1]) * d,
        a[2] + (b[2] - a[2]) * d,
        a[3] + (b[3] - a[3]) * d,
    ]
}

pub fn blink_color(base: [f32; 4], start: Instant) -> Option<[f32; 4]> {
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let elapsed = start.elapsed();
    let end = Duration::from_millis(1000);
    let mid = end / 2;

    if elapsed < mid {
        let d = (mid - elapsed).subsec_millis() as f32 / mid.subsec_millis() as f32;
        Some(interp4(base, WHITE, d))
    } else if elapsed < end {
        let d = (end - elapsed).subsec_millis() as f32 / mid.subsec_millis() as f32;
        Some(interp4(WHITE, base, d))
    } else {
        None
    }
}
