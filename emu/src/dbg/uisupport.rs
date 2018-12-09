use imgui::sys;
use imgui::*;
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

pub fn imgui_input_hex(ui: &Ui<'_>, name: &ImStr, val: &mut u64, wait_enter: bool) -> bool {
    let mut changed = false;
    ui.with_item_width(70.0, || {
        let mut spc = ImString::new(format!("{:08x}", val));
        if ui
            .input_text(name, &mut spc)
            .chars_hexadecimal(true)
            .enter_returns_true(wait_enter)
            .auto_select_all(true)
            .build()
        {
            *val = u64::from_str_radix(spc.as_ref(), 16).unwrap();
            changed = true;
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
