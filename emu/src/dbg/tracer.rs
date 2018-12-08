use array_macro::array;
use rustc_hash::FxHashMap;

use crate::bus::{AccessSize, MemInt};

use std::cell::Cell;
use std::time::Instant;

const DEBUGGER_MAX_CPU: usize = 8;

#[derive(Copy, Clone, Debug)]
pub enum TraceEvent {
    Poll(), // Fake event used to poll back into the tracer to improve responsiveness
    Breakpoint(usize, u64), // A breakpoint was hit at the specified PC
    Breakhere(usize, u64), // A break-here was hit at the specified PC
    WatchpointWrite(usize, u64), // A watchpoint was hit during a write at the specified address
    WatchpointRead(usize, u64), // A watchpoint was hit during a read at the specifiied address
    GenericBreak(&'static str), // Another kind of condition was hit, and we want to stop the tracing.
}

pub type Result = std::result::Result<(), TraceEvent>;

/// A tracer is a debugger that can do fine-grained tracing of an emulator
/// and trigger specific events when a certain condition is reached (eg: a
/// breakpoint is hit).
/// It is passed as argument to DebuggerModel::trace, as entry-point for
/// debugger tracing.
pub trait Tracer {
    fn trace_insn(&self, cpu_idx: usize, pc: u64) -> Result;
    fn trace_mem_write(&self, cpu_idx: usize, addr: u64, size: AccessSize, val: u64) -> Result;
    fn trace_mem_read(&self, cpu_idx: usize, addr: u64, size: AccessSize, val: u64) -> Result;
    fn trace_gpu(&self, line: usize) -> Result;
}

// A dummy tracer that never returns a tracing event.
pub struct NullTracer {}

impl Tracer for NullTracer {
    fn trace_insn(&self, _cpu_idx: usize, _pc: u64) -> Result {
        Ok(())
    }
    fn trace_mem_write(&self, _cpu_idx: usize, _addr: u64, _size: AccessSize, _val: u64) -> Result {
        Ok(())
    }
    fn trace_mem_read(&self, _cpu_idx: usize, _addr: u64, _size: AccessSize, _val: u64) -> Result {
        Ok(())
    }
    fn trace_gpu(&self, _line: usize) -> Result {
        Ok(())
    }
}

pub(crate) struct Breakpoint {
    active: bool,
    _description: String,
}

#[derive(Copy, Clone)]
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

pub(crate) struct Watchpoint {
    active: bool,
    _description: String,
    condition: WatchpointCondition,
}

#[derive(Default)]
struct DbgCpu {
    breakpoints: FxHashMap<u64, Breakpoint>,
    read_watchpoints: FxHashMap<u64, Watchpoint>,
    write_watchpoints: FxHashMap<u64, Watchpoint>,
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
}

impl Tracer for Debugger {
    fn trace_insn(&self, cpu_idx: usize, pc: u64) -> Result {
        match self.cpus[cpu_idx].breakpoints.get(&pc) {
            Some(ref bp) if bp.active => Err(TraceEvent::Breakpoint(cpu_idx, pc)),
            _ => Ok(()),
        }
    }

    fn trace_mem_read(&self, cpu_idx: usize, addr: u64, _size: AccessSize, val: u64) -> Result {
        match self.cpus[cpu_idx].read_watchpoints.get(&addr) {
            Some(ref wp) if wp.active && wp.condition.check(val) => {
                Err(TraceEvent::WatchpointRead(cpu_idx, addr))
            }
            _ => Ok(()),
        }
    }

    fn trace_mem_write(&self, cpu_idx: usize, addr: u64, _size: AccessSize, val: u64) -> Result {
        match self.cpus[cpu_idx].write_watchpoints.get(&addr) {
            Some(ref wp) if wp.active && wp.condition.check(val) => {
                Err(TraceEvent::WatchpointWrite(cpu_idx, addr))
            }
            _ => Ok(()),
        }
    }

    fn trace_gpu(&self, _line: usize) -> Result {
        // Check if the polling interval is elapsed. Do this only every line
        // (not every insn or memory access, since otherwise the overhead is
        // too big).
        if let Some(poll_when) = self.next_poll.get() {
            if poll_when <= Instant::now() {
                self.next_poll.set(None);
                return Err(TraceEvent::Poll());
            }
        }
        Ok(())
    }
}
