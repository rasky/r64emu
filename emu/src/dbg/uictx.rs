use super::{TraceEvent, DEBUGGER_MAX_CPU};
use imgui::ImString;

use std::time::Instant;

#[derive(Default)]
pub(crate) struct UiCtxDisasm {
    pub blink_pc: Option<(u64, Instant)>,
}

// Global state shared by all debugger UIs, passed to all rendere functions.
//
// This is useful for two main reasons:
// 1) Keep local state of a ImgUi window; in C++, this is done with static variables,
// but in Rust we need to store it in a different way.
// 2) Propagate cross-window information (eg: specific events that affect multiple windows).
#[derive(Default)]
pub(crate) struct UiCtx {
    // An event that was just triggered. This is kept only for one frame.
    pub event: Option<(Box<TraceEvent>, Instant)>,

    // Disasm views
    pub disasm: [UiCtxDisasm; DEBUGGER_MAX_CPU],

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
