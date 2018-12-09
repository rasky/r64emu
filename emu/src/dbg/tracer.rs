use super::uisupport::imgui_input_hex;
use array_macro::array;
use imgui::*;

use crate::bus::{AccessSize, MemInt};

use std::cell::Cell;
use std::cmp::Ordering;
use std::time::Instant;

const DEBUGGER_MAX_CPU: usize = 8;

#[derive(Debug)]
pub enum TraceEvent {
    Poll(), // Fake event used to poll back into the tracer to improve responsiveness
    Breakpoint(usize, usize), // A breakpoint was hit (cpu_idx, bp_idx)
    Breakhere(usize), // A break-here was hit
    WatchpointWrite(usize, usize), // A watchpoint was hit during a write (cpu_idx, wp_idx)
    WatchpointRead(usize, usize), // A watchpoint was hit during a read (cpu_idx, wp_idx)
    GenericBreak(&'static str), // Another kind of condition was hit, and we want to stop the tracing.
}

pub type Result<T> = std::result::Result<T, Box<TraceEvent>>;

/// A tracer is a debugger that can do fine-grained tracing of an emulator
/// and trigger specific events when a certain condition is reached (eg: a
/// breakpoint is hit).
/// It is passed as argument to DebuggerModel::trace, as entry-point for
/// debugger tracing.
pub trait Tracer {
    fn trace_insn(&self, cpu_idx: usize, pc: u64) -> Result<()>;
    fn trace_mem_write(&self, cpu_idx: usize, addr: u64, size: AccessSize, val: u64) -> Result<()>;
    fn trace_mem_read(&self, cpu_idx: usize, addr: u64, size: AccessSize, val: u64) -> Result<()>;
    fn trace_gpu(&self, line: usize) -> Result<()>;
}

// A dummy tracer that never returns a tracing event.
pub struct NullTracer {}

impl Tracer for NullTracer {
    fn trace_insn(&self, _cpu_idx: usize, _pc: u64) -> Result<()> {
        Ok(())
    }
    fn trace_mem_write(
        &self,
        _cpu_idx: usize,
        _addr: u64,
        _size: AccessSize,
        _val: u64,
    ) -> Result<()> {
        Ok(())
    }
    fn trace_mem_read(
        &self,
        _cpu_idx: usize,
        _addr: u64,
        _size: AccessSize,
        _val: u64,
    ) -> Result<()> {
        Ok(())
    }
    fn trace_gpu(&self, _line: usize) -> Result<()> {
        Ok(())
    }
}

#[derive(Eq)]
pub(crate) struct Breakpoint {
    active: bool,
    pc: u64,
    description: String,
}

impl Ord for Breakpoint {
    fn cmp(&self, other: &Self) -> Ordering {
        self.pc.cmp(&other.pc)
    }
}

impl PartialOrd for Breakpoint {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Breakpoint {
    fn eq(&self, other: &Self) -> bool {
        self.pc == other.pc
    }
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub(crate) enum WatchpointType {
    Read,
    Write,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum WatchpointCondition {
    Always,
    Eq(u64), // equal to
    Ne(u64), // not equal to
    Gt(u64), // greater than
    Ge(u64), // greater or equal than
    Lt(u64), // less than
    Le(u64), // less or equal than
}

impl WatchpointCondition {
    fn check<T: MemInt>(&self, value: T) -> bool {
        use self::WatchpointCondition::*;
        let value: u64 = value.into();
        match *self {
            Always => true,
            Eq(cmp) => value == cmp,
            Ne(cmp) => value != cmp,
            Gt(cmp) => value > cmp,
            Ge(cmp) => value >= cmp,
            Lt(cmp) => value < cmp,
            Le(cmp) => value <= cmp,
        }
    }
}

#[derive(Eq)]
pub(crate) struct Watchpoint {
    active: bool,
    addr: u64,
    wtype: WatchpointType,
    condition: WatchpointCondition,
    description: String,
}

impl Watchpoint {
    fn cond_to_string(&self) -> String {
        use self::WatchpointCondition::*;
        use self::WatchpointType::*;
        match self.wtype {
            Read => match self.condition {
                Always => format!("Any read"),
                Eq(cmp) => format!("Value read == 0x{:x}", cmp),
                Ne(cmp) => format!("Value read != 0x{:x}", cmp),
                Gt(cmp) => format!("Value read > 0x{:x}", cmp),
                Ge(cmp) => format!("Value read >= 0x{:x}", cmp),
                Lt(cmp) => format!("Value read < 0x{:x}", cmp),
                Le(cmp) => format!("Value read <= 0x{:x}", cmp),
            },
            Write => match self.condition {
                Always => format!("Any write"),
                Eq(cmp) => format!("Value written == 0x{:x}", cmp),
                Ne(cmp) => format!("Value written != 0x{:x}", cmp),
                Gt(cmp) => format!("Value written > 0x{:x}", cmp),
                Ge(cmp) => format!("Value written >= 0x{:x}", cmp),
                Lt(cmp) => format!("Value written < 0x{:x}", cmp),
                Le(cmp) => format!("Value written <= 0x{:x}", cmp),
            },
        }
    }
}

impl Ord for Watchpoint {
    fn cmp(&self, other: &Self) -> Ordering {
        self.addr
            .cmp(&other.addr)
            .then(self.wtype.cmp(&other.wtype))
    }
}

impl PartialOrd for Watchpoint {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Watchpoint {
    fn eq(&self, other: &Self) -> bool {
        self.addr == other.addr && self.wtype == other.wtype
    }
}

#[derive(Default)]
struct DbgCpu {
    breakpoints: Vec<Breakpoint>,
    watchpoints: Vec<Watchpoint>,

    bp_fastmap: IntHashMap<u64, usize>,
    wp_fastmap: IntHashMap<u64, usize>,

    ui: DebuggerUi,
}

impl DbgCpu {
    fn add_breakpoint(&mut self, pc: u64, description: &str) {
        self.breakpoints.push(Breakpoint {
            active: true,
            pc: pc,
            description: description.to_owned(),
        });
        self.breakpoints.sort();
        self.update_bp_fastmap();
    }
    fn add_watchpoint(
        &mut self,
        addr: u64,
        description: &str,
        wtype: WatchpointType,
        condition: WatchpointCondition,
    ) {
        self.watchpoints.push(Watchpoint {
            active: true,
            addr,
            description: description.to_owned(),
            wtype,
            condition,
        });
        self.breakpoints.sort();
        self.update_wp_fastmap();
    }

    fn update_bp_fastmap(&mut self) {
        self.bp_fastmap = self
            .breakpoints
            .iter()
            .enumerate()
            .filter(|(_, bp)| bp.active)
            .map(|(idx, bp)| (bp.pc, idx))
            .collect();
    }
    fn update_wp_fastmap(&mut self) {
        self.wp_fastmap = self
            .watchpoints
            .iter()
            .enumerate()
            .filter(|(_, wp)| wp.active)
            .map(|(idx, wp)| (wp.addr, idx))
            .collect();
    }
}

pub struct Debugger {
    cpus: [DbgCpu; DEBUGGER_MAX_CPU],
    next_poll: Cell<Option<Instant>>,
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            cpus: array![DbgCpu::default(); DEBUGGER_MAX_CPU],
            next_poll: Cell::new(None),
        }
    }

    pub fn set_poll_event(&mut self, when: Instant) {
        self.next_poll.set(Some(when));
    }

    pub fn add_breakpoint(&mut self, cpu_idx: usize, pc: u64, description: &str) {
        self.cpus[cpu_idx].add_breakpoint(pc, description);
    }
}

impl Tracer for Debugger {
    fn trace_insn(&self, cpu_idx: usize, pc: u64) -> Result<()> {
        match self.cpus[cpu_idx].bp_fastmap.get(&pc) {
            Some(idx) => Err(box TraceEvent::Breakpoint(cpu_idx, *idx)),
            None => Ok(()),
        }
    }

    fn trace_mem_read(&self, cpu_idx: usize, addr: u64, _size: AccessSize, val: u64) -> Result<()> {
        let cpu = &self.cpus[cpu_idx];
        match cpu.wp_fastmap.get(&addr) {
            Some(idx) => {
                let wp = &cpu.watchpoints[*idx];
                if wp.wtype == WatchpointType::Read && wp.condition.check(val) {
                    Err(box TraceEvent::WatchpointRead(cpu_idx, *idx))
                } else {
                    Ok(())
                }
            }
            None => Ok(()),
        }
    }

    fn trace_mem_write(
        &self,
        cpu_idx: usize,
        addr: u64,
        _size: AccessSize,
        val: u64,
    ) -> Result<()> {
        let cpu = &self.cpus[cpu_idx];
        match cpu.wp_fastmap.get(&addr) {
            Some(idx) => {
                let wp = &cpu.watchpoints[*idx];
                if wp.wtype == WatchpointType::Write && wp.condition.check(val) {
                    Err(box TraceEvent::WatchpointRead(cpu_idx, *idx))
                } else {
                    Ok(())
                }
            }
            None => Ok(()),
        }
    }

    fn trace_gpu(&self, _line: usize) -> Result<()> {
        // Check if the polling interval is elapsed. Do this only every line
        // (not every insn or memory access, since otherwise the overhead is
        // too big).
        if let Some(poll_when) = self.next_poll.get() {
            if poll_when <= Instant::now() {
                self.next_poll.set(None);
                return Err(box TraceEvent::Poll());
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct DebuggerUi {
    new_bp_pc: u64,
    new_bp_desc: ImString,

    new_wp_addr: u64,
    new_wp_desc: ImString,
    new_wp_type: i32,
    new_wp_cond: i32,
    new_wp_value: u64,
}

impl Debugger {
    fn render_breakpoints(&mut self, ui: &Ui<'_>, cpu_idx: usize) {
        let cpu = &mut self.cpus[cpu_idx];

        ui.popup(im_str!("##bp#new"), || {
            ui.text(im_str!("PC:"));
            ui.same_line(60.0);
            imgui_input_hex(ui, im_str!("###bp#new_pc"), &mut cpu.ui.new_bp_pc);

            ui.text(im_str!("Desc:"));
            ui.same_line(60.0);
            ui.input_text(im_str!("###bp#new_desc"), &mut cpu.ui.new_bp_desc)
                .auto_select_all(true)
                .build();

            if ui.button(im_str!("Add"), (40.0, 20.0)) {
                let desc = cpu.ui.new_bp_desc.to_str().to_owned();
                cpu.add_breakpoint(cpu.ui.new_bp_pc, &desc);
                ui.close_current_popup();
            }
        });
        if ui.small_button(im_str!("New BP")) {
            cpu.ui.new_bp_pc = 0;
            cpu.ui.new_bp_desc = ImString::new("New breakpoint");
            ui.open_popup(im_str!("##bp#new"));
        }

        let mut bp_changed = false;

        ui.columns(4, im_str!(""), true);
        ui.set_column_offset(1, 30.0);
        ui.set_column_offset(2, 110.0);
        for (idx, bp) in cpu.breakpoints.iter_mut().enumerate() {
            let name = im_str!("###breakpoints#active#{}", idx);
            if ui.checkbox(name, &mut bp.active) {
                // Changing activation requires update to fastmap
                bp_changed = true;
            }
            ui.next_column();

            let name = im_str!("###breakpoints#pc#{}", idx);
            if imgui_input_hex(ui, name, &mut bp.pc) {
                // Changing PC requires update to fastmap
                bp_changed = true;
            }
            ui.next_column();

            let name = im_str!("###breakpoints#desc#{}", idx);
            let mut sdesc = ImString::new(bp.description.clone());
            if ui
                .input_text(name, &mut sdesc)
                .enter_returns_true(true)
                .auto_select_all(true)
                .build()
            {
                bp.description = sdesc.to_str().to_owned();
            }
            ui.next_column();

            ui.text(im_str!("{}", "Always"));
            ui.next_column();
        }
        ui.columns(1, im_str!(""), false);

        // Refresh breakpoint hashmap if required
        if bp_changed {
            cpu.update_bp_fastmap();
        }
    }

    fn render_watchpoints(&mut self, ui: &Ui<'_>, cpu_idx: usize) {
        let cpu = &mut self.cpus[cpu_idx];

        ui.popup(im_str!("##wp#new"), || {
            ui.text(im_str!("Address:"));
            ui.same_line(80.0);
            imgui_input_hex(ui, im_str!("###wp#new_addr"), &mut cpu.ui.new_wp_addr);

            ui.text(im_str!("Desc:"));
            ui.same_line(80.0);
            ui.input_text(im_str!("###wp#new_desc"), &mut cpu.ui.new_wp_desc)
                .auto_select_all(true)
                .build();

            ui.text(im_str!("Type:"));
            ui.same_line(80.0);
            ui.radio_button(im_str!("Read"), &mut cpu.ui.new_wp_type, 0);
            ui.same_line(150.0);
            ui.radio_button(im_str!("Write"), &mut cpu.ui.new_wp_type, 1);

            ui.text(im_str!("Condition:"));
            ui.same_line(80.0);
            ui.combo(
                im_str!("###wp#new_cond"),
                &mut cpu.ui.new_wp_cond,
                &[
                    im_str!("Always"),
                    im_str!("=="),
                    im_str!("!="),
                    im_str!(">="),
                    im_str!("<="),
                    im_str!(">"),
                    im_str!("<"),
                ],
                0,
            );

            if cpu.ui.new_wp_cond != 0 {
                ui.text(im_str!("Value:"));
                ui.same_line(80.0);
                imgui_input_hex(ui, im_str!("###wp#new_value"), &mut cpu.ui.new_wp_value);
            }

            if ui.button(im_str!("Add"), (40.0, 20.0)) {
                let desc = cpu.ui.new_wp_desc.to_str().to_owned();
                let wtype = if cpu.ui.new_wp_type == 0 {
                    WatchpointType::Read
                } else {
                    WatchpointType::Write
                };
                let cond = match cpu.ui.new_wp_cond {
                    0 => WatchpointCondition::Always,
                    1 => WatchpointCondition::Eq(cpu.ui.new_wp_value),
                    2 => WatchpointCondition::Ne(cpu.ui.new_wp_value),
                    3 => WatchpointCondition::Ge(cpu.ui.new_wp_value),
                    4 => WatchpointCondition::Le(cpu.ui.new_wp_value),
                    5 => WatchpointCondition::Gt(cpu.ui.new_wp_value),
                    6 => WatchpointCondition::Lt(cpu.ui.new_wp_value),
                    _ => unreachable!(),
                };
                cpu.add_watchpoint(cpu.ui.new_wp_addr, &desc, wtype, cond);
                ui.close_current_popup();
            }
        });
        if ui.small_button(im_str!("New WP")) {
            cpu.ui.new_wp_addr = 0;
            cpu.ui.new_wp_desc = ImString::new("New watchpoint");
            cpu.ui.new_wp_type = 0;
            cpu.ui.new_wp_cond = 0;
            cpu.ui.new_wp_value = 0;
            ui.open_popup(im_str!("##wp#new"));
        }

        let mut wp_changed = false;

        ui.columns(4, im_str!(""), true);
        ui.set_column_offset(1, 30.0);
        ui.set_column_offset(2, 110.0);
        for (idx, wp) in cpu.watchpoints.iter_mut().enumerate() {
            let name = im_str!("###watchpoints#active#{}", idx);
            if ui.checkbox(name, &mut wp.active) {
                // Changing activation requires update to fastmap
                wp_changed = true;
            }
            ui.next_column();

            let name = im_str!("###watchpoints#addr#{}", idx);
            if imgui_input_hex(ui, name, &mut wp.addr) {
                // Changing addr requires update to fastmap
                wp_changed = true;
            }
            ui.next_column();

            let name = im_str!("###watchpoint#desc#{}", idx);
            let mut sdesc = ImString::new(wp.description.clone());
            if ui
                .input_text(name, &mut sdesc)
                .enter_returns_true(true)
                .auto_select_all(true)
                .build()
            {
                wp.description = sdesc.to_str().to_owned();
            }
            ui.next_column();

            ui.text(im_str!("{}", wp.cond_to_string()));
            ui.next_column();
        }
        ui.columns(1, im_str!(""), false);

        // Refresh breakpoint hashmap if required
        if wp_changed {
            cpu.update_wp_fastmap();
        }
    }

    fn render_points(&mut self, ui: &Ui<'_>) {
        ui.window(im_str!("Breakpoints & Watchpoints"))
            .size((200.0, 400.0), ImGuiCond::FirstUseEver)
            .build(|| {
                let cpu_idx: usize = 0;
                if ui
                    .collapsing_header(im_str!("Breakpoints"))
                    .default_open(true)
                    .build()
                {
                    self.render_breakpoints(ui, cpu_idx);
                }
                if ui
                    .collapsing_header(im_str!("Watchpoints"))
                    .default_open(true)
                    .build()
                {
                    self.render_watchpoints(ui, cpu_idx);
                }
            });
    }

    pub fn render_main(&mut self, ui: &Ui<'_>) {
        self.render_points(ui);
    }
}

use self::inthashmap::IntHashMap;
mod inthashmap {
    // Simple integer hasher from:
    // https://users.rust-lang.org/t/hashmap-performance/6476/14
    // https://gist.github.com/arthurprs/88eef0b57b9f8341c54e2d82ec775698
    use std::hash::Hasher;
    pub struct SimpleHasher(u64);

    #[inline]
    fn load_u64_le(buf: &[u8], len: usize) -> u64 {
        use std::ptr;
        debug_assert!(len <= buf.len());
        let mut data = 0u64;
        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), &mut data as *mut _ as *mut u8, len);
        }
        data.to_le()
    }

    impl Default for SimpleHasher {
        #[inline]
        fn default() -> SimpleHasher {
            SimpleHasher(0)
        }
    }

    impl Hasher for SimpleHasher {
        #[inline]
        fn finish(&self) -> u64 {
            self.0
        }

        #[inline]
        fn write(&mut self, bytes: &[u8]) {
            *self = SimpleHasher(load_u64_le(bytes, bytes.len()));
        }
    }

    use std::collections::HashMap;
    use std::hash::BuildHasherDefault;
    pub type IntHashMap<K, V> = HashMap<K, V, BuildHasherDefault<SimpleHasher>>;
}
