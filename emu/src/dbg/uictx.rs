use super::TraceEvent;
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

#[derive(Default)]
pub(crate) struct UiCtxDisasm {
    pub blink_pc: Option<(u64, Instant)>,
    pub cursor_pc: Option<u64>,
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

    // Popup "New breakpoint": local state
    pub new_bp_pc: u64,
    pub new_bp_desc: ImString,

    // Popup "New watchpoint": local state
    pub new_wp_addr: u64,
    pub new_wp_desc: ImString,
    pub new_wp_type: i32,
    pub new_wp_cond: i32,
    pub new_wp_value: u64,
}
