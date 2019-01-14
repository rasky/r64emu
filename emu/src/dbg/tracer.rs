use super::uisupport::imgui_input_hex;
use super::UiCtx;
use array_macro::array;
use bitflags::bitflags;
use imgui::*;

use crate::memint::{AccessSize, MemInt};

use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub enum TraceEvent {
    Poll(),    // Fake event used to poll back into the tracer to improve responsiveness
    Paused(),  // A pause was requested
    Stepped(), // A CPU just stepped
    Breakpoint(String, usize, u64), // A breakpoint was hit (cpu_idx, bp_idx, pc)
    BreakpointOneShot(String, u64), // A one-shot breakpoint was hit (cpu_idx, pc)
    WatchpointWrite(String, usize), // A watchpoint was hit during a write (cpu_idx, wp_idx)
    WatchpointRead(String, usize), // A watchpoint was hit during a read (cpu_idx, wp_idx)
    GenericBreak(String), // Another kind of condition was hit, and we want to stop the tracing.
}

pub type Result<T> = std::result::Result<T, Box<TraceEvent>>;

bitflags! {
    struct TraceGuard: u8 {
        const INSN      = 0b00000001;
        const MEM_READ  = 0b00000010;
        const MEM_WRITE = 0b00000100;
    }
}

impl TraceGuard {
    fn index<T: MemInt>(addr: T) -> usize {
        let addr: u64 = addr.into();
        ((addr >> 2) & 0xFF) as usize
    }
}

/// A tracer is a debugger that can do fine-grained tracing of an emulator
/// and trigger specific events when a certain condition is reached (eg: a
/// breakpoint is hit).
/// It is passed as argument to DebuggerModel::trace, as entry-point for
/// debugger tracing.
pub struct Tracer<'a> {
    dbg: Option<&'a Debugger>,
    trace_guards: [TraceGuard; 256],
}

impl Tracer<'_> {
    /// Create a null tracer, not connected to a real debugger. All operations
    /// will be nops.
    pub fn null() -> Tracer<'static> {
        Tracer {
            dbg: None,
            trace_guards: array![TraceGuard::empty(); 256],
        }
    }

    #[inline(always)]
    pub fn break_here(&self, msg: &str) -> Result<()> {
        if self.dbg.is_none() {
            return Ok(());
        }
        Err(box TraceEvent::GenericBreak(msg.to_owned()))
    }

    #[inline(always)]
    pub fn panic(&self, msg: &str) -> Result<()> {
        if self.dbg.is_none() {
            panic!("{}", msg);
        }
        Err(box TraceEvent::GenericBreak(msg.to_owned()))
    }

    #[inline(always)]
    pub fn trace_gpu(&self, line: usize) -> Result<()> {
        self.dbg.map(|t| t.trace_gpu(line)).unwrap_or(Ok(()))
    }

    #[inline(always)]
    pub fn trace_insn(&self, cpu_name: &str, pc: u64) -> Result<()> {
        if self.dbg.is_none() {
            return Ok(());
        }
        if self.trace_guards[TraceGuard::index(pc)].contains(TraceGuard::INSN) {
            self.dbg.unwrap().trace_insn(cpu_name, pc)
        } else {
            Ok(())
        }
    }

    #[inline(always)]
    pub fn trace_mem_write(
        &self,
        cpu_name: &str,
        addr: u64,
        size: AccessSize,
        val: u64,
    ) -> Result<()> {
        if self.dbg.is_none() {
            return Ok(());
        }
        if self.trace_guards[TraceGuard::index(addr)].contains(TraceGuard::MEM_WRITE) {
            self.dbg.unwrap().trace_mem_write(cpu_name, addr, size, val)
        } else {
            Ok(())
        }
    }

    #[inline(always)]
    pub fn trace_mem_read(
        &self,
        cpu_name: &str,
        addr: u64,
        size: AccessSize,
        val: u64,
    ) -> Result<()> {
        if self.dbg.is_none() {
            return Ok(());
        }
        if self.trace_guards[TraceGuard::index(addr)].contains(TraceGuard::MEM_READ) {
            self.dbg.unwrap().trace_mem_read(cpu_name, addr, size, val)
        } else {
            Ok(())
        }
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

    bp_oneshot: Option<u64>, // Special one-shot breakpoint

    bp_fastmap: IntHashMap<u64, usize>,
    wp_fastmap: IntHashMap<u64, usize>,
}

impl DbgCpu {
    fn add_breakpoint(&mut self, pc: u64, description: &str) {
        self.breakpoints.push(Breakpoint {
            active: true,
            pc: pc,
            description: description.to_owned(),
        });
        self.update_bp_fastmap();
    }

    fn set_breakpoint_oneshot(&mut self, pc: Option<u64>) {
        self.bp_oneshot = pc;
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
        self.update_wp_fastmap();
    }

    fn update_bp_fastmap(&mut self) {
        self.breakpoints.sort();
        self.bp_fastmap = self
            .breakpoints
            .iter()
            .enumerate()
            .filter(|(_, bp)| bp.active)
            .map(|(idx, bp)| (bp.pc, idx))
            .collect();
    }
    fn update_wp_fastmap(&mut self) {
        self.breakpoints.sort();
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
    cpus: HashMap<String, DbgCpu>,
    next_poll: Cell<Option<Instant>>,
}

impl Debugger {
    pub fn new(cpus: &Vec<String>) -> Self {
        let mut cpumap = HashMap::new();
        for name in cpus {
            cpumap.insert(name.clone(), DbgCpu::default());
        }

        Self {
            cpus: cpumap,
            next_poll: Cell::new(None),
        }
    }

    pub fn set_breakpoint_oneshot(&mut self, cpu_name: &str, pc: Option<u64>) {
        self.cpus
            .get_mut(cpu_name)
            .unwrap()
            .set_breakpoint_oneshot(pc);
    }

    pub fn disable_breakpoint_oneshot(&mut self) {
        for (_, cpu) in &mut self.cpus {
            cpu.set_breakpoint_oneshot(None);
        }
    }

    pub fn set_poll_event(&mut self, when: Instant) {
        self.next_poll.set(Some(when));
    }

    pub fn add_breakpoint(&mut self, cpu_name: &str, pc: u64, description: &str) {
        self.cpus
            .get_mut(cpu_name)
            .unwrap()
            .add_breakpoint(pc, description);
    }
}

impl Debugger {
    pub fn new_tracer(&self) -> Tracer {
        let mut trace_guards = array![TraceGuard::empty(); 256];
        for (_, cpu) in &self.cpus {
            for bp in &cpu.breakpoints {
                trace_guards[TraceGuard::index(bp.pc)].insert(TraceGuard::INSN);
            }
            if let Some(pc) = cpu.bp_oneshot {
                trace_guards[TraceGuard::index(pc)].insert(TraceGuard::INSN);
            }
            for wp in &cpu.watchpoints {
                trace_guards[TraceGuard::index(wp.addr)].insert(match wp.wtype {
                    WatchpointType::Read => TraceGuard::MEM_READ,
                    WatchpointType::Write => TraceGuard::MEM_WRITE,
                });
            }
        }
        Tracer {
            dbg: Some(&self),
            trace_guards: trace_guards,
        }
    }

    fn trace_insn(&self, cpu_name: &str, pc: u64) -> Result<()> {
        let cpu = &self.cpus[cpu_name];
        match cpu.bp_fastmap.get(&pc) {
            Some(idx) => Err(box TraceEvent::Breakpoint(cpu_name.to_owned(), *idx, pc)),
            None => match cpu.bp_oneshot {
                Some(bp_pc) if bp_pc == pc => {
                    Err(box TraceEvent::BreakpointOneShot(cpu_name.to_owned(), pc))
                }
                _ => Ok(()),
            },
        }
    }

    fn trace_mem_read(&self, cpu_name: &str, addr: u64, _size: AccessSize, val: u64) -> Result<()> {
        let cpu = &self.cpus[cpu_name];
        match cpu.wp_fastmap.get(&addr) {
            Some(idx) => {
                let wp = &cpu.watchpoints[*idx];
                if wp.wtype == WatchpointType::Read && wp.condition.check(val) {
                    Err(box TraceEvent::WatchpointRead(cpu_name.to_owned(), *idx))
                } else {
                    Ok(())
                }
            }
            None => Ok(()),
        }
    }

    fn trace_mem_write(
        &self,
        cpu_name: &str,
        addr: u64,
        _size: AccessSize,
        val: u64,
    ) -> Result<()> {
        let cpu = &self.cpus[cpu_name];
        match cpu.wp_fastmap.get(&addr) {
            Some(idx) => {
                let wp = &cpu.watchpoints[*idx];
                if wp.wtype == WatchpointType::Write && wp.condition.check(val) {
                    Err(box TraceEvent::WatchpointWrite(cpu_name.to_owned(), *idx))
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

impl Debugger {
    fn render_breakpoints(&mut self, ui: &Ui<'_>, ctx: &mut UiCtx, cpu_name: &str) {
        let cpu = self.cpus.get_mut(cpu_name).unwrap();

        ui.popup(im_str!("##bp#new"), || {
            ui.text(im_str!("PC:"));
            ui.same_line(60.0);
            imgui_input_hex(ui, im_str!("###bp#new_pc"), &mut ctx.new_bp_pc, false);

            ui.text(im_str!("Desc:"));
            ui.same_line(60.0);
            ui.input_text(im_str!("###bp#new_desc"), &mut ctx.new_bp_desc)
                .auto_select_all(true)
                .build();

            if ui.button(im_str!("Add"), (40.0, 20.0)) {
                let desc = ctx.new_bp_desc.to_str().to_owned();
                cpu.add_breakpoint(ctx.new_bp_pc, &desc);
                ui.close_current_popup();
            }
        });
        if ui.small_button(im_str!("New BP")) {
            ctx.new_bp_pc = 0;
            ctx.new_bp_desc = ImString::new("New breakpoint");
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
            if imgui_input_hex(ui, name, &mut bp.pc, true) {
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

    fn render_watchpoints(&mut self, ui: &Ui<'_>, ctx: &mut UiCtx, cpu_name: &str) {
        let cpu = self.cpus.get_mut(cpu_name).unwrap();

        ui.popup(im_str!("##wp#new"), || {
            ui.text(im_str!("Address:"));
            ui.same_line(80.0);
            imgui_input_hex(ui, im_str!("###wp#new_addr"), &mut ctx.new_wp_addr, false);

            ui.text(im_str!("Desc:"));
            ui.same_line(80.0);
            ui.input_text(im_str!("###wp#new_desc"), &mut ctx.new_wp_desc)
                .auto_select_all(true)
                .build();

            ui.text(im_str!("Type:"));
            ui.same_line(80.0);
            ui.radio_button(im_str!("Read"), &mut ctx.new_wp_type, 0);
            ui.same_line(150.0);
            ui.radio_button(im_str!("Write"), &mut ctx.new_wp_type, 1);

            ui.text(im_str!("Condition:"));
            ui.same_line(80.0);
            ui.combo(
                im_str!("###wp#new_cond"),
                &mut ctx.new_wp_cond,
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

            if ctx.new_wp_cond != 0 {
                ui.text(im_str!("Value:"));
                ui.same_line(80.0);
                imgui_input_hex(ui, im_str!("###wp#new_value"), &mut ctx.new_wp_value, false);
            }

            if ui.button(im_str!("Add"), (40.0, 20.0)) {
                let desc = ctx.new_wp_desc.to_str().to_owned();
                let wtype = if ctx.new_wp_type == 0 {
                    WatchpointType::Read
                } else {
                    WatchpointType::Write
                };
                let cond = match ctx.new_wp_cond {
                    0 => WatchpointCondition::Always,
                    1 => WatchpointCondition::Eq(ctx.new_wp_value),
                    2 => WatchpointCondition::Ne(ctx.new_wp_value),
                    3 => WatchpointCondition::Ge(ctx.new_wp_value),
                    4 => WatchpointCondition::Le(ctx.new_wp_value),
                    5 => WatchpointCondition::Gt(ctx.new_wp_value),
                    6 => WatchpointCondition::Lt(ctx.new_wp_value),
                    _ => unreachable!(),
                };
                cpu.add_watchpoint(ctx.new_wp_addr, &desc, wtype, cond);
                ui.close_current_popup();
            }
        });
        if ui.small_button(im_str!("New WP")) {
            ctx.new_wp_addr = 0;
            ctx.new_wp_desc = ImString::new("New watchpoint");
            ctx.new_wp_type = 0;
            ctx.new_wp_cond = 0;
            ctx.new_wp_value = 0;
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
            if imgui_input_hex(ui, name, &mut wp.addr, true) {
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

    fn render_points(&mut self, ui: &Ui<'_>, ctx: &mut UiCtx) {
        for idx in 0..ctx.cpus.len() {
            let cpu_name = ctx.cpus[idx].clone();

            ui.window(im_str!("[{}] Breakpoints & Watchpoints", cpu_name))
                .size((200.0, 400.0), ImGuiCond::FirstUseEver)
                .build(|| {
                    if ui
                        .collapsing_header(im_str!("Breakpoints"))
                        .default_open(true)
                        .build()
                    {
                        self.render_breakpoints(ui, ctx, &cpu_name);
                    }
                    if ui
                        .collapsing_header(im_str!("Watchpoints"))
                        .default_open(true)
                        .build()
                    {
                        self.render_watchpoints(ui, ctx, &cpu_name);
                    }
                });
        }
    }

    pub(crate) fn render_main(&mut self, ui: &Ui<'_>, ctx: &mut UiCtx) {
        self.render_points(ui, ctx);
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
