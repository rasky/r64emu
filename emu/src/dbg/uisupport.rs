use imgui::sys;
use imgui::*;
use std::fmt;
use std::time::{Duration, Instant};

pub(crate) struct ImGuiListClipper {
    items_count: usize,
    items_height: f32,
}

pub(crate) struct ImGuiListClipperToken {
    clip: sys::ImGuiListClipper,
    end: bool,
}

impl ImGuiListClipperToken {
    pub(crate) fn display_start(&self) -> usize {
        return self.clip.DisplayStart as usize;
    }
    pub(crate) fn display_end(&self) -> usize {
        return self.clip.DisplayEnd as usize;
    }

    pub(crate) fn step(&mut self) -> bool {
        unsafe { sys::ImGuiListClipper_Step(&mut self.clip) }
    }

    pub(crate) fn run<F: FnMut(isize, isize)>(&mut self, mut f: F) {
        loop {
            if !self.step() {
                break;
            }
            f(
                self.clip.DisplayStart as isize,
                self.clip.DisplayEnd as isize,
            );
        }
    }

    pub(crate) fn end(mut self) {
        unsafe {
            sys::ImGuiListClipper_End(&mut self.clip);
        }
        self.end = true;
    }
}

impl Drop for ImGuiListClipperToken {
    fn drop(&mut self) {
        if !self.end {
            panic!("A ImGuiListClipperToken was leaked. Did you call .end()?");
        }
    }
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

    pub(crate) fn begin(self) -> ImGuiListClipperToken {
        let mut clip: sys::ImGuiListClipper = unsafe { ::std::mem::uninitialized() };
        unsafe {
            sys::ImGuiListClipper_Begin(&mut clip, self.items_count as _, self.items_height as _);
        }
        ImGuiListClipperToken { clip, end: false }
    }

    pub(crate) fn build<F: FnMut(isize, isize)>(self, f: F) {
        let mut tok = self.begin();
        tok.run(f);
        tok.end();
    }
}

pub trait HexableInt: Copy + fmt::Display {
    const HEX_DIGITS: usize;
    fn format(self) -> String;
    fn parse(s: &str) -> Option<Self>;
    fn as_i64(self) -> i64;
}

impl HexableInt for u8 {
    const HEX_DIGITS: usize = 2;
    fn format(self) -> String {
        format!("{:02x}", self)
    }
    fn parse(s: &str) -> Option<Self> {
        u8::from_str_radix(s, 16).ok()
    }
    fn as_i64(self) -> i64 {
        self as i8 as i64
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
    fn as_i64(self) -> i64 {
        self as i16 as i64
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
    fn as_i64(self) -> i64 {
        self as i32 as i64
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
    fn as_i64(self) -> i64 {
        self as i64
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
    if ui.is_item_hovered() {
        let mut binary = String::new();
        let mut ascii = String::new();
        let v64 = val.as_i64();
        for i in (0..T::HEX_DIGITS / 2).rev() {
            if !binary.is_empty() {
                binary += &format!("{:10}  ", " ");
            }
            let v8 = ((v64 >> (i * 8)) & 0xFF) as u8;
            binary += &format!(
                "{:04b} {:04b}  [{}..{}]\n",
                v8 >> 4,
                v8 & 0xF,
                (i * 8) + 7,
                (i * 8)
            );
            ascii.push(if v8.is_ascii_control() {
                '.'
            } else {
                v8 as char
            });
        }

        ui.tooltip_text(im_str!(
            "{:10}: {}\n{:10}: {}\n{:10}: {}\n{:10}: {}\n",
            "Unsigned",
            val,
            "Signed",
            val.as_i64(),
            "Ascii",
            ascii,
            "Binary",
            binary,
        ));
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

pub fn is_shortcut_pressed(ui: &Ui, key: u32) -> bool {
    !ui.io().want_text_input
        && ui.is_window_focused_with_flags(WindowFocusedFlags::ROOT_AND_CHILD_WINDOWS)
        && ui.is_key_pressed(key)
}

pub fn ctext(ui: &Ui, text: &ImStr, id: i32) {
    let pt = ui.push_id(id);
    ui.text(text);
    ui.popup(im_str!("##context"), || {
        if MenuItem::new(im_str!("Copy")).build(&ui) {
            println!("Copied: {}", text);
        }
    });
    if ui.is_item_hovered() && ui.is_item_clicked(MouseButton::Right) {
        ui.open_popup(im_str!("##context"));
    }
    pt.pop(&ui);
}
