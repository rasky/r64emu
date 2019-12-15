use super::uisupport::ImGuiListClipper;
use crate::bus;
use crate::memint::ByteOrderCombiner;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use imgui::*;
use imgui_sys::igCaptureKeyboardFromApp;
use sdl2::keyboard::Scancode;

use std::borrow::Cow;

const NUM_COLUMNS: usize = 16;

/// MemoryBank describes a single memory bank exposed by a [`MemoryView`](trait.MemoryView.html).
/// It contains
pub struct MemoryBank {
    /// Name of the bank (exposed in the user interface)
    name: String,
    /// Address of the first byte of the bank. This is only used to display the
    /// memory bank using addresses which are familiar in the context of the emulator
    /// (eg: addresses in which those memory ares are mapped when accessed by a CPU).
    begin: u64,
    /// Address of the last byte of the bank (inclusive bound).
    end: u64,
    /// If true, the user will be allowed to modify the memory bank within the debugger.
    /// This might or might not correspond to the memory bank being writable by
    /// emulated CPUs; for instance, one might want to make a ROM bank being editable
    /// by the user in the dbeugger.
    rw: bool,
}

impl MemoryBank {
    /// Construct a MemoryBank instance
    pub fn new(name: &str, begin: u64, end: u64, rw: bool) -> MemoryBank {
        MemoryBank {
            name: name.to_string(),
            begin,
            end,
            rw,
        }
    }

    fn size(&self) -> usize {
        (self.end - self.begin + 1) as usize
    }
    fn clamp(&self, addr: u64) -> u64 {
        addr.min(self.end).max(self.begin)
    }
}

/// MemoryView is a trait implemented by objects that can be inspected within
/// the memory view window of the debugger.
///
/// Each memory view is defined as a list of memory banks of contiguous memory.
/// Each bank is described by an instance of [`MemoryBank`](struct.MemoryBank.html),
/// and can be accessed using the [`mem_slice`](fn.MemoryView.mem_slice.html) or
/// [`mem_slice_mut`](fn.MemoryView.mem_slice_mut.html) methods of this trait.
pub trait MemoryView {
    /// Return the name of this view
    fn name(&self) -> &str;

    /// Return the memory banks this view is composed of.
    ///
    /// NOTE: the debugger does not currently work correctly if the returned
    /// value changes during the runtime (eg: new banks are added/removed). Please
    /// make sure this function returns a constant value across frames, for the
    /// whole execution of the debugger.
    fn banks(&self) -> Vec<MemoryBank>;

    /// Get a non mutable reference to a slice of one of the memory banks.
    fn mem_slice<'a>(&'a self, bank_idx: usize, start: u64, end: u64) -> &'a [u8];

    /// Get a mutable reference to a slice of one of the memory banks.
    fn mem_slice_mut<'a>(&'a mut self, bank_idx: usize, start: u64, end: u64) -> &'a mut [u8];
}

#[derive(Default)]
pub(crate) struct MemWindow {
    contents_width_changed: bool,
    curr_bank: usize,                   // current bank within MemoryView
    edit_addr: Option<u64>,             // address currently being edited by user (if any)
    inspect_addr: Option<u64>,          // address currently inspected in footer (if any)
    inspect_size: usize,                // size in bytes of the memory being inspected
    highlight_addr: Option<(u64, u64)>, // bytes currently highlighted in view
    force_addr: Option<u64>,            // address that user requested to go to
    edit_buf: ImString, // edit buffer used by input box to hold data written by user
    edit_addr_focus: bool, // if true, this frame the edit input box must take focus
    inspect_type: usize, // type of inspection (u8, i16, etc.)
    inspect_endian: usize, // endianess of inspection
}

#[derive(Default, Debug)]
struct Sizes {
    addr_digits_count: usize, // number of hex digits to print in the address column
    line_height: f32,         // height of a line
    glyph_width: f32,         // width of a char (monospace assumed)
    hex_cell_width: f32,      // size of a single hex cell, including spacing (2 bytes)
    hex_cell_spacing: f32,    // spacing between hex cells
    spacing_between_mid_cols: f32, // central spacing between different 8-byte sequences
    pos_hex_start: f32,       // X position where the hex dump starts
    pos_hex_end: f32,         // X poistion where the hex dump ends
    pos_ascii_start: f32,     // X position where the ASCII dump starts
    pos_ascii_end: f32,       // X position where the ASCII dump ends
    window_width: f32,        // total window width
}

fn color(r: usize, g: usize, b: usize) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

impl MemWindow {
    fn calc_sizes(&self, ui: &Ui, addr: u64) -> Sizes {
        let style = ui.clone_style();
        let mut s = Sizes::default();
        s.addr_digits_count = ((64 - addr.leading_zeros() as usize) + 3) / 4;
        s.line_height = ui.text_line_height();
        s.glyph_width = ui.calc_text_size(im_str!("F"), false, 0.0)[0];
        s.hex_cell_spacing = s.glyph_width * 0.5;
        s.hex_cell_width = s.glyph_width * 2.0 + s.hex_cell_spacing;
        s.spacing_between_mid_cols = (s.hex_cell_width + s.hex_cell_spacing) * 0.25;
        s.pos_hex_start = (s.addr_digits_count + 2) as f32 * s.glyph_width;
        s.pos_hex_end =
            s.pos_hex_start + s.hex_cell_width * NUM_COLUMNS as f32 + s.spacing_between_mid_cols;
        s.pos_ascii_start = s.pos_hex_end + s.glyph_width;
        s.pos_ascii_end = s.pos_ascii_start + NUM_COLUMNS as f32 * s.glyph_width;
        s.window_width =
            s.pos_ascii_end + style.scrollbar_size + style.window_padding[0] * 2.0 + s.glyph_width;
        return s;
    }

    pub(crate) fn render(&mut self, ui: &Ui, memview: &mut dyn MemoryView) {
        let banks = memview.banks();
        let bank = &banks[self.curr_bank];
        let s = self.calc_sizes(ui, bank.end);

        Window::new(&im_str!("[{}]: Memory view", memview.name()))
            .scroll_bar(false)
            .size([s.window_width, 300.0], Condition::FirstUseEver)
            .size_constraints([0.0, 0.0], [s.window_width, 1e+9])
            .build(ui, || {
                let mut curr_bank = self.curr_bank;

                ui.set_next_item_width(130.0);
                ComboBox::new(im_str!("")).build_simple(
                    ui,
                    &mut curr_bank,
                    &banks,
                    &|b: &MemoryBank| Cow::Owned(im_str!("{}", b.name)),
                );
                ui.same_line(0.0);

                if ui.button(im_str!("Goto.."), [0.0, 0.0]) {
                    ui.open_popup(im_str!("##goto"));
                }
                ui.popup(im_str!("##goto"), || {
                    let mut s = ImString::new("00000000");
                    ui.text(im_str!("Address:"));
                    if ui
                        .input_text(im_str!("##input"), &mut s)
                        .chars_hexadecimal(true)
                        .enter_returns_true(true)
                        .auto_select_all(true)
                        .build()
                    {
                        self.force_addr = u64::from_str_radix(s.as_ref(), 16).ok();
                        self.edit_addr = None;
                        self.inspect_addr = None;
                        self.highlight_addr = None;
                        ui.close_current_popup();
                    }
                });

                // Render main hex view area
                self.render_contents(ui, memview, &s);
                ui.separator();

                // Footer
                self.render_footer(ui, memview, &s);

                self.curr_bank = curr_bank;
            });
    }

    fn render_contents(&mut self, ui: &Ui, memview: &mut dyn MemoryView, s: &Sizes) {
        let banks = memview.banks();
        let bank = &banks[self.curr_bank];

        let style = ui.clone_style();
        let height_separator = style.item_spacing[1];
        let mut footer_height = 0.0;

        let color_disabled = ui.style_color(StyleColor::TextDisabled);
        let color_text = ui.style_color(StyleColor::Text);
        let color_border = ui.style_color(StyleColor::Border);
        let color_hightlight = [1.0, 1.0, 1.0, 0.2];

        footer_height += height_separator + ui.frame_height_with_spacing(); // options
        footer_height += height_separator
            + ui.frame_height_with_spacing()
            + ui.text_line_height_with_spacing() * 3.0; // data preview
        ChildWindow::new(im_str!("##scrolling"))
            .size([0.0, -footer_height])
            .flags(WindowFlags::NO_MOVE)
            .build(ui, || {
                let st = ui.push_style_vars(&[
                    StyleVar::FramePadding([0.0, 0.0]),
                    StyleVar::ItemSpacing([0.0, 0.0]),
                ]);

                let num_lines = (bank.size() + NUM_COLUMNS - 1) / NUM_COLUMNS;

                let mut clip = ImGuiListClipper::new(num_lines)
                    .items_height(s.line_height)
                    .begin();
                let visible_start = clip.display_start();
                let visible_end = clip.display_end();
                let screen_lines = visible_end - visible_start;

                if let Some(mut addr) = self.edit_addr {
                    let prev_addr = addr;

                    if ui.is_window_focused_with_flags(WindowFocusedFlags::ROOT_AND_CHILD_WINDOWS) {
                        if ui.is_key_pressed(ui.key_index(Key::UpArrow)) {
                            addr = addr.saturating_sub(NUM_COLUMNS as u64);
                            self.edit_addr_focus = true;
                        }
                        if ui.is_key_pressed(ui.key_index(Key::DownArrow)) {
                            addr = addr.saturating_add(NUM_COLUMNS as u64);
                            self.edit_addr_focus = true;
                        }
                        if ui.is_key_pressed(ui.key_index(Key::LeftArrow)) {
                            addr = addr.saturating_sub(1);
                            self.edit_addr_focus = true;
                        }
                        if ui.is_key_pressed(ui.key_index(Key::RightArrow)) {
                            addr = addr.saturating_add(1);
                            self.edit_addr_focus = true;
                        }
                        if ui.is_key_pressed(Scancode::PageUp as _) {
                            addr = addr.saturating_add((screen_lines * NUM_COLUMNS) as u64);
                            self.edit_addr_focus = true;
                        }
                        if ui.is_key_pressed(Scancode::PageDown as _) {
                            addr = addr.saturating_add((screen_lines * NUM_COLUMNS) as u64);
                            self.edit_addr_focus = true;
                        }
                    }
                    addr = bank.clamp(addr);
                    self.edit_addr = Some(addr);
                    if let Some((mut h1, mut h2)) = self.highlight_addr {
                        let hsize = h2 - h1 + 1;
                        if addr < h1 {
                            h1 -= (h1 - addr + hsize - 1) / hsize * hsize;
                        } else if addr > h2 {
                            h1 += (addr - h2 + hsize - 1) / hsize * hsize;
                        }
                        h2 = h1 + hsize - 1;
                        self.highlight_addr = Some((h1, h2));
                        self.inspect_addr = Some(h1);
                    }

                    let addr_line = addr as usize / NUM_COLUMNS;
                    let prev_addr_line = prev_addr as usize / NUM_COLUMNS;
                    if (addr < prev_addr && addr_line < visible_start + 2)
                        || (addr > prev_addr && addr_line > visible_end - 3)
                    {
                        ui.set_scroll_y(
                            ui.scroll_y()
                                + (addr_line.wrapping_sub(prev_addr_line) as i64) as f32
                                    * s.line_height,
                        );
                    }
                }

                if let Some(mut addr) = self.force_addr {
                    addr = bank.clamp(addr);

                    let addr_line = addr as usize / NUM_COLUMNS;
                    if addr_line < visible_start + 2 || addr_line > visible_end - 2 {
                        ui.set_scroll_y(
                            addr_line.wrapping_sub(screen_lines / 2) as f32 * s.line_height,
                        );
                    }
                    self.force_addr = None;
                }

                let window_pos = ui.window_pos();
                let dl = ui.get_window_draw_list();
                let addr_separator_x = window_pos[0] + s.pos_hex_start - s.glyph_width;
                let ascii_separator_x = window_pos[0] + s.pos_ascii_start - s.glyph_width;
                dl.add_line(
                    [addr_separator_x, window_pos[1]],
                    [addr_separator_x, window_pos[1] + 1e5],
                    color_border,
                )
                .build();
                dl.add_line(
                    [ascii_separator_x, window_pos[1]],
                    [ascii_separator_x, window_pos[1] + 1e5],
                    color_border,
                )
                .build();

                clip.run(|start, end| {
                    for line in start..end {
                        let line_addr_start = bank.begin + (line as u64 * NUM_COLUMNS as u64);
                        let line_addr_end =
                            (line_addr_start + (NUM_COLUMNS as u64) - 1).min(bank.end);

                        let mem =
                            memview.mem_slice_mut(self.curr_bank, line_addr_start, line_addr_end);
                        ui.text_colored(
                            color(174, 129, 255),
                            &im_str!("{:01$X}", line_addr_start, s.addr_digits_count),
                        );

                        for addr in line_addr_start..=line_addr_end {
                            let n = (addr - line_addr_start) as usize;
                            ui.same_line(
                                s.pos_hex_start
                                    + n as f32 * s.hex_cell_width
                                    + (n / 8) as f32 * s.spacing_between_mid_cols,
                            );

                            // Start of highlighting
                            if let Some((h1, h2)) = self.highlight_addr {
                                if addr == h1 {
                                    let hsize = (h2 - h1 + 1) as usize;
                                    let pos = ui.cursor_screen_pos();
                                    let ncells = hsize.min(NUM_COLUMNS - n);
                                    let has_mid =
                                        (n < NUM_COLUMNS / 2) && (n + ncells > NUM_COLUMNS / 2);
                                    let width = ncells as f32 * s.hex_cell_width
                                        - s.hex_cell_spacing
                                        + if has_mid {
                                            s.spacing_between_mid_cols
                                        } else {
                                            0.0
                                        };
                                    dl.add_rect(
                                        pos,
                                        [pos[0] + width, pos[1] + s.line_height],
                                        color_hightlight,
                                    )
                                    .filled(true)
                                    .build();
                                }
                            }

                            if self.edit_addr == Some(addr) {
                                let idt = ui.push_id((addr) as i32);
                                let iwt = ui.push_item_width(s.glyph_width * 2.0);
                                self.edit_buf = ImString::with_capacity(2);
                                self.edit_buf.push_str(&format!("{:02X}", mem[n]));

                                if self.edit_addr_focus {
                                    ui.set_keyboard_focus_here(FocusedWidget::Previous);
                                    unsafe {
                                        igCaptureKeyboardFromApp(true);
                                    }
                                    self.edit_addr_focus = false;
                                }

                                if ui
                                    .input_text(im_str!("##data"), &mut self.edit_buf)
                                    .auto_select_all(true)
                                    .chars_hexadecimal(true)
                                    .chars_uppercase(true)
                                    .enter_returns_true(true)
                                    .no_horizontal_scroll(true)
                                    .always_insert_mode(true)
                                    .resize_buffer(false)
                                    .build()
                                {
                                    mem[n] =
                                        u8::from_str_radix(self.edit_buf.to_str(), 16).unwrap();
                                    self.edit_addr = Some(self.edit_addr.unwrap() + 1);
                                    self.edit_addr_focus = true;
                                }

                                iwt.pop(&ui);
                                idt.pop(&ui);
                            } else {
                                let val = mem[n];
                                if val == 0 {
                                    ui.text_disabled(&im_str!("{:02X}", mem[n]));
                                } else {
                                    ui.text(&im_str!("{:02X}", mem[n]));
                                }
                                if bank.rw
                                    && ui.is_item_hovered()
                                    && ui.is_mouse_clicked(MouseButton::Left)
                                {
                                    self.inspect_addr = Some(addr);
                                    self.edit_addr = Some(addr);
                                    self.edit_addr_focus = true;
                                    self.highlight_addr = Some((
                                        addr,
                                        addr.saturating_add(self.inspect_size as u64 - 1),
                                    ));
                                }
                            }
                        }

                        ui.same_line(s.pos_ascii_start);
                        let mut pos = ui.cursor_screen_pos();

                        let aid = ui.push_id(start as i32);
                        if ui.invisible_button(
                            im_str!("##ascii"),
                            [s.pos_ascii_end - s.pos_ascii_start, s.line_height],
                        ) {}
                        aid.pop(ui);

                        for val in mem {
                            if *val < 32 || *val >= 128 {
                                dl.add_text(pos, color_disabled, ".");
                            } else {
                                dl.add_text(pos, color_text, (*val as char).to_string());
                            }
                            pos[0] += s.glyph_width;
                        }
                    }
                });
                clip.end();
                st.pop(&ui);
            });
    }

    fn render_footer(&mut self, ui: &Ui, memview: &mut dyn MemoryView, s: &Sizes) {
        let isizes: [&ImStr; 8] = [
            im_str!("Uint8"),
            im_str!("Uint16"),
            im_str!("Uint32"),
            im_str!("Uint64"),
            im_str!("Int8"),
            im_str!("Int16"),
            im_str!("Int32"),
            im_str!("Int64"),
        ];
        let endians: [&ImStr; 2] = [im_str!("LE"), im_str!("BE")];
        let style = ui.clone_style();
        let mut update_highlight = false;

        ui.align_text_to_frame_padding();
        ui.text(im_str!("Inspect as:"));
        ui.same_line(0.0);
        ui.set_next_item_width(
            s.glyph_width * 10.0 + style.frame_padding[0] * 2.0 + style.item_inner_spacing[0],
        );
        if ComboBox::new(im_str!("##type"))
            .height(ComboBoxHeight::Largest)
            .build_simple_string(ui, &mut self.inspect_type, &isizes)
        {
            update_highlight = true;
        }
        ui.same_line(0.0);
        ui.set_next_item_width(
            s.glyph_width * 6.0 + style.frame_padding[0] * 2.0 + style.item_inner_spacing[0],
        );
        ComboBox::new(im_str!("##endian")).build_simple_string(
            ui,
            &mut self.inspect_endian,
            &endians,
        );

        self.inspect_size = 1 << (self.inspect_type & 7);

        if let Some(addr) = self.inspect_addr {
            // Try to read enough bytes for the inspection, and ignore it if
            // it's not possible (eg: we're at the end of the buffer).
            let mem = memview.mem_slice(self.curr_bank, addr, addr + self.inspect_size as u64 - 1);
            if mem.len() == self.inspect_size {
                let (dec, hex, bin) = self.render_inspect_type(mem);
                let x = s.glyph_width * 6.0;

                ui.text(im_str!("Dec:"));
                ui.same_line(x);
                ui.text(im_str!("{}", dec));

                ui.text(im_str!("Hex:"));
                ui.same_line(x);
                for (i, v) in hex.as_bytes().chunks(4).enumerate() {
                    if i != 0 {
                        ui.same_line(0.0);
                    }
                    ui.text(im_str!("{}", std::str::from_utf8_unchecked(v)));
                }

                ui.text(im_str!("Bin:"));
                ui.same_line(x);
                for (i, v) in bin.as_bytes().chunks(4).enumerate() {
                    if i != 0 {
                        if (i & 7) == 0 {
                            ui.text(im_str!("")); // force newline
                            ui.same_line(x);
                        } else {
                            ui.same_line(0.0);
                        }
                    }
                    ui.text(im_str!("{}", std::str::from_utf8_unchecked(v)));
                }

                if update_highlight {
                    self.highlight_addr =
                        Some((addr, addr.saturating_add(self.inspect_size as u64 - 1)));
                }
            }
        }
    }

    fn render_inspect_type(&self, mem: &[u8]) -> (String, String, String) {
        match self.inspect_type {
            0 | 4 => {
                let val = mem[0];
                if self.inspect_type == 0 {
                    (
                        format!("{}", val),
                        format!("{:02x}", val),
                        format!("{:08b}", val),
                    )
                } else {
                    (
                        format!("{}", val as i8),
                        format!("{:02x}", val as i8),
                        format!("{:08b}", val as i8),
                    )
                }
            }
            1 | 5 => {
                let val = if self.inspect_endian == 0 {
                    LittleEndian::read_u16(mem)
                } else {
                    BigEndian::read_u16(mem)
                };
                if self.inspect_type == 1 {
                    (
                        format!("{}", val),
                        format!("{:04x}", val),
                        format!("{:016b}", val),
                    )
                } else {
                    (
                        format!("{}", val as i16),
                        format!("{:04x}", val as i16),
                        format!("{:016b}", val as i16),
                    )
                }
            }
            2 | 6 => {
                let val = if self.inspect_endian == 0 {
                    LittleEndian::read_u32(mem)
                } else {
                    BigEndian::read_u32(mem)
                };
                if self.inspect_type == 1 {
                    (
                        format!("{}", val),
                        format!("{:08x}", val),
                        format!("{:032b}", val),
                    )
                } else {
                    (
                        format!("{}", val as i32),
                        format!("{:08x}", val as i32),
                        format!("{:032b}", val as i32),
                    )
                }
            }
            3 | 7 => {
                let val = if self.inspect_endian == 0 {
                    LittleEndian::read_u64(mem)
                } else {
                    BigEndian::read_u64(mem)
                };
                if self.inspect_type == 1 {
                    (
                        format!("{}", val),
                        format!("{:016x}", val),
                        format!("{:064b}", val),
                    )
                } else {
                    (
                        format!("{}", val as i64),
                        format!("{:016x}", val as i64),
                        format!("{:064b}", val as i64),
                    )
                }
            }
            _ => unreachable!(),
        }
    }
}

/// BusMemoryView is a trait that helps implementing [`MemoryView`](trait.MemoryView.html)
/// through a emu::bus::Bus object. All objects implementing `BusMemoryView` also
/// automatically implements `MemoryView`, so it can used as a simpler alternative in
/// case you want to expose all banks that are mapped in a `Bus`.
pub trait BusMemoryView {
    /// Order is the byte order of the bus
    type Order: ByteOrderCombiner + 'static;

    /// Name of the memory view. For instance, it could be the name of the CPU
    /// that the bus is attached to.
    fn name(&self) -> &str;

    /// Return a reference to the bus. All memory areas mapped in the bus
    /// will be available for debugging.
    fn bus(&self) -> &bus::Bus<Self::Order>;

    /// Return a mutable reference to the bus.
    fn bus_mut(&mut self) -> &mut bus::Bus<Self::Order>;
}

impl<T: BusMemoryView> MemoryView for T {
    fn name(&self) -> &str {
        BusMemoryView::name(self)
    }

    fn banks(&self) -> Vec<MemoryBank> {
        self.bus()
            .mapped_mems()
            .iter()
            .filter_map(|b| {
                return if !b.readable {
                    None
                } else {
                    Some(MemoryBank::new(&b.name, b.begin, b.end, b.writeable))
                };
            })
            .collect()
    }

    fn mem_slice(&self, _bank_idx: usize, start: u64, end: u64) -> &[u8] {
        &self
            .bus()
            .fetch_read_nolog::<u8>(start as u32)
            .mem()
            .unwrap()[..=(end - start) as usize]
    }

    fn mem_slice_mut(&mut self, _bank_idx: usize, start: u64, end: u64) -> &mut [u8] {
        &mut self
            .bus_mut()
            .fetch_write_nolog::<u8>(start as u32)
            .mem()
            .unwrap()[..=(end - start) as usize]
    }
}
