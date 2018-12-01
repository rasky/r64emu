extern crate imgui;
use self::imgui::*;

pub enum RegisterSize<'a> {
    Reg8(&'a mut u8),
    Reg16(&'a mut u16),
    Reg32(&'a mut u32),
    Reg64(&'a mut u64),
}

/// A trait for an object that can display register contents to
/// a debugger view.
pub trait RegisterView {
    fn name<'a>(&'a self) -> &'a str;
    fn visit_regs<'s, F>(&'s mut self, visit: F)
    where
        F: for<'a> FnMut(&'a str, RegisterSize<'a>);
}

pub(crate) fn render_regview<'a, 'ui, RV: RegisterView>(ui: &'a Ui<'ui>, v: &mut RV) {
    ui.window(im_str!("[{}] Register View", v.name()))
        .size((200.0, 900.0), ImGuiCond::FirstUseEver)
        .build(|| {
            v.visit_regs(|name, val| {
                let name = im_str!("{}", name);
                let mut buf = match val {
                    RegisterSize::Reg8(v) => im_str!("{:02x}", v).to_owned(),
                    RegisterSize::Reg16(v) => im_str!("{:04x}", v).to_owned(),
                    RegisterSize::Reg32(v) => im_str!("{:08x}", v).to_owned(),
                    RegisterSize::Reg64(v) => im_str!("{:016x}", v).to_owned(),
                };
                ui.input_text(name, &mut buf).build();
            })
        });
}
