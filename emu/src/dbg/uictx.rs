use super::TraceEvent;
use crate::log::{LogLine, LogView};
use imgui::ImString;

use std::collections::HashMap;
use std::time::Instant;

// UiCommand is an action triggered by the GUI that is executed
// by the main debugger loop (cannot be done while drawing the window)
pub(crate) enum UiCommand {
    BreakpointOneShot(String, u64), // Run with a temporary breakpoint set
    CpuStep(String),                // Step a single opcode for the specified CPU
    Pause(bool),                    // Set global pause status
}

pub(crate) enum RegHighlight {
    Input,
    Output,
}

// Global state for a disasm view
#[derive(Default)]
pub(crate) struct UiCtxDisasm {
    // Current PC on this CPU (copied here from emulator).
    pub cur_pc: Option<u64>,
    // Display a blinking effect over this PC; initialize with Instant::now.
    // NOTE: this only works if this PC is visible (within the scrolling area), otherwise
    // the animation is not performed.
    pub blink_pc: Option<(u64, Instant)>,
    // PC being currently selected by the user using cursor keys / mouse.
    pub cursor_pc: Option<u64>,
    // If set, the disasmview will automatically scroll to display this PC
    pub force_pc: Option<u64>,
    // Map of registers that must be highlighted (because are involved in cur_pc's opcode).
    pub regs_highlight: HashMap<&'static str, RegHighlight>,
}

// A command that can be requested by a log view (returned
// by the render function).
pub(crate) enum LogViewCommand {
    // User requested to see a certain PC in a specific CPU
    ShowPc(String, u64),
}

// Global state for log view
pub(crate) struct UiCtxLog {
    pub view: LogView,
    pub name: String,
    pub cached_lines: Vec<LogLine>,
    pub cached_start_line: usize,
    pub last_filter_count: Instant,
    pub filter_count: Option<usize>,
    pub following: bool,
    pub configured_columns: bool,
    pub selected: LogLine,
    pub opened: bool,
}

impl UiCtxLog {
    pub(crate) fn new(view: LogView, name: &str) -> Box<UiCtxLog> {
        Box::new(UiCtxLog {
            view,
            name: name.to_owned(),
            last_filter_count: Instant::now(),
            cached_lines: Vec::new(),
            cached_start_line: 0,
            filter_count: None,
            selected: LogLine::default(),
            following: true,
            configured_columns: false,
            opened: true,
        })
    }
}

// Global state shared by all debugger UIs, passed to all rendere functions.
//
// This is useful for two main reasons:
// 1) Keep local state of a ImgUi window; in C++, this is done with static variables,
// but in Rust we need to store it in a different way.
// 2) Propagate cross-window information (eg: specific events that affect multiple windows).
#[derive(Default)]
pub(crate) struct UiCtx {
    pub cpus: Vec<String>,

    // An event that was just triggered. This is kept only for one frame.
    pub event: Option<(Box<TraceEvent>, Instant)>,

    // A command requested by the UI to the debugger
    pub command: Option<UiCommand>,

    // Disasm views
    pub disasm: HashMap<String, UiCtxDisasm>,

    // Log view
    pub logviews: Vec<Box<UiCtxLog>>,
    pub logviewid: usize,

    // Flash messages (auto-hide after 2s)
    pub flash_msg: Option<(String, Instant)>,

    // Error message that will be displayed in a modal
    pub error_msg: Option<String>,

    // Popup "New breakpoint": local state
    pub new_bp_pc: u64,
    pub new_bp_desc: ImString,

    // Popup "New watchpoint": local state
    pub new_wp_addr: u64,
    pub new_wp_desc: ImString,
    pub new_wp_type: i32,
    pub new_wp_cond: usize,
    pub new_wp_value: u64,
}

impl UiCtx {
    pub fn add_flash_msg(&mut self, msg: &str) {
        self.flash_msg = Some((msg.to_owned(), Instant::now()));
    }
}
