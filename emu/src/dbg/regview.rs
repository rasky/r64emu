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
    const WINDOW_SIZE: (f32, f32);
    const COLUMNS: usize;
    fn name<'a>(&'a self) -> &'a str;
    fn visit_regs<'s, F>(&'s mut self, col: usize, visit: F)
    where
        F: for<'a> FnMut(&'a str, RegisterSize<'a>);
}

pub(crate) fn render_regview<'a, 'ui, RV: RegisterView>(ui: &'a Ui<'ui>, v: &mut RV) {
    ui.window(im_str!("[{}] Registers", v.name()))
        .size(RV::WINDOW_SIZE, ImGuiCond::FirstUseEver)
        .build(|| {
            ui.columns(RV::COLUMNS as _, im_str!("columns"), true);
            for col in 0..RV::COLUMNS {
                v.visit_regs(col, |name, val| {
                    let name = im_str!("{}", name);
                    let mut buf = match val {
                        RegisterSize::Reg8(ref v) => im_str!("{:02x}", v).to_owned(),
                        RegisterSize::Reg16(ref v) => im_str!("{:04x}", v).to_owned(),
                        RegisterSize::Reg32(ref v) => im_str!("{:08x}", v).to_owned(),
                        RegisterSize::Reg64(ref v) => im_str!("{:016x}", v).to_owned(),
                    };
                    if ui
                        .input_text(name, &mut buf)
                        .chars_hexadecimal(true)
                        .chars_noblank(true)
                        .enter_returns_true(true)
                        .build()
                    {
                        match val {
                            RegisterSize::Reg8(v) => {
                                *v = u8::from_str_radix(buf.as_ref(), 16).unwrap();
                            }
                            RegisterSize::Reg16(v) => {
                                *v = u16::from_str_radix(buf.as_ref(), 16).unwrap();
                            }
                            RegisterSize::Reg32(v) => {
                                *v = u32::from_str_radix(buf.as_ref(), 16).unwrap();
                            }
                            RegisterSize::Reg64(v) => {
                                *v = u64::from_str_radix(buf.as_ref(), 16).unwrap();
                            }
                        };
                    }
                });
                ui.next_column();
            }
        });
}
