extern crate imgui;
use self::imgui::*;

/// A trait for an object that can display register contents to
/// a debugger view.
pub trait DisasmView {
    /// Return the name of this object. The name will be composed
    /// as "[NAME] Disassembly".
    fn name(&self) -> &str;

    /// Return the current program counter.
    fn pc(&self) -> u64;

    /// Return the currently-valid range for the program counter.
    fn pc_range(&self) -> (u64, u64);

    /// Disassemble a single instruction at the specified program counter;
    /// Returns the bytes composing the instruction and the string representation.
    fn disasm_block<Func: Fn(u64, &[u8], &str)>(&self, pc_range: (u64, u64), f: Func);
}

struct ByteBuf<'a>(&'a [u8]);

impl<'a> std::fmt::LowerHex for ByteBuf<'a> {
    fn fmt(&self, fmtr: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        for byte in self.0 {
            fmtr.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
    }
}
pub(crate) fn render_disasmview<'a, 'ui, DV: DisasmView>(ui: &'a Ui<'ui>, v: &mut DV) {
    ui.window(im_str!("[{}] Disassembly", v.name()))
        .size((450.0, 400.0), ImGuiCond::FirstUseEver)
        .build(|| {
            let pc = 0x1000; //v.pc() - 16 * 4;
            v.disasm_block((pc, pc + 32 * 4), |pc, mem, text| {
                ui.text(im_str!("{:08x}", pc));
                ui.same_line(80.0);
                ui.text(im_str!("{:x}", ByteBuf(mem)));
                ui.same_line(170.0);
                ui.text(im_str!("{}", text));
            });
        });
}
