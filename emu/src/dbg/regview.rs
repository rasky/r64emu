use super::uisupport::*;
use super::UiCtx;
use imgui::*;

pub enum RegisterSize<'a> {
    Reg8(&'a mut u8),
    Reg16(&'a mut u16),
    Reg32(&'a mut u32),
    Reg64(&'a mut u64),
}

/// A trait for an object that can display register contents to
/// a debugger view.
pub trait RegisterView {
    const WINDOW_SIZE: (f32, f32);
    const COLUMNS: usize;
    fn name<'a>(&'a self) -> &'a str;
    fn visit_regs<'s, F>(&'s mut self, col: usize, visit: F)
    where
        F: for<'a> FnMut(&'a str, RegisterSize<'a>, Option<&str>);
}

pub(crate) fn render_regview<'a, 'ui, RV: RegisterView>(
    ui: &'a Ui<'ui>,
    _ctx: &mut UiCtx,
    v: &mut RV,
) {
    ui.window(im_str!("[{}] Registers", v.name()))
        .size(RV::WINDOW_SIZE, ImGuiCond::FirstUseEver)
        .build(|| {
            ui.columns(RV::COLUMNS as _, im_str!("columns"), true);
            for col in 0..RV::COLUMNS {
                v.visit_regs(col, |name, val, desc| {
                    use self::RegisterSize::*;
                    let name = im_str!("{}", name);
                    match val {
                        Reg8(v) => imgui_input_hex(ui, name, v, true),
                        Reg16(v) => imgui_input_hex(ui, name, v, true),
                        Reg32(v) => imgui_input_hex(ui, name, v, true),
                        Reg64(v) => imgui_input_hex(ui, name, v, true),
                    };
                    if let Some(desc) = desc {
                        ui.text(im_str!("{}", desc));
                    }
                });
                ui.next_column();
            }
        });
}
