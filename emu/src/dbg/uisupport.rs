extern crate imgui;

use self::imgui::sys;

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
