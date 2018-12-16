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
            f(clip.display_start as isize, clip.display_end as isize);
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
    ui.with_item_width(T::HEX_DIGITS as f32 * 7.0 + 8.0, || {
        let mut spc = ImString::new(val.format());
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
    });
    changed
}

fn interp4(a: ImVec4, b: ImVec4, d: f32) -> ImVec4 {
    ImVec4::new(
        a.x + (b.x - a.x) * d,
        a.y + (b.y - a.y) * d,
        a.z + (b.z - a.z) * d,
        a.w + (b.w - a.w) * d,
    )
}

pub fn blink_color(base: ImVec4, start: Instant) -> Option<ImVec4> {
    let elapsed = start.elapsed();
    let white = ImVec4::new(1.0, 1.0, 1.0, 1.0);
    let end = Duration::from_millis(1000);
    let mid = end / 2;

    if elapsed < mid {
        let d = (mid - elapsed).subsec_millis() as f32 / mid.subsec_millis() as f32;
        Some(interp4(base, white, d))
    } else if elapsed < end {
        let d = (end - elapsed).subsec_millis() as f32 / mid.subsec_millis() as f32;
        Some(interp4(white, base, d))
    } else {
        None
    }
}
