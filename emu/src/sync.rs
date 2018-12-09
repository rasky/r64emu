use slog::*;

use crate::dbg;
use crate::int::Numerics;

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    HSync(usize, usize),
    VSync(usize, usize),
}

pub struct Config {
    pub main_clock: i64,
    pub dot_clock_divider: i64,
    pub hdots: usize,
    pub vdots: usize,
    pub hsyncs: Vec<usize>,
    pub vsyncs: Vec<usize>,
}

pub trait Subsystem {
    // Run the subsytem until the specified target of cycles is reached.
    // Optionally, report events to the specified tracer (debugger).
    fn run(&mut self, target_cycles: i64, tracer: Option<&dyn dbg::Tracer>) -> dbg::Result<()>;

    // Return the current number of cycles elapsed in the subsytem.
    // Notice that this might be called from within run(), and it's supposed
    // to always hold the updated value
    fn cycles(&self) -> i64;

    // Return the program counter for this subsystem (if any)
    fn pc(&self) -> Option<u64>;
}

type SubPtr = Rc<RefCell<dyn Subsystem>>;

#[derive(Clone)]
struct SubInfo {
    name: String,
    sub: SubPtr,
    scaler: f64,
}

pub struct Sync {
    pub cfg: Config,
    subs: Vec<SubInfo>,
    current_sub: Option<*const dyn Subsystem>,
    current_subinfo: Option<SubInfo>,
    logger: slog::Logger,

    frames: i64,
    cycles: i64,
    line_cycles: i64,
    frame_cycles: i64,
    frame_syncs: Vec<(i64, Event)>,
    curr_frame: Option<(i64, usize)>,
}

impl Sync {
    pub fn new(logger: slog::Logger, cfg: Config) -> Box<Sync> {
        let mut s = Box::new(Sync {
            cfg,
            logger: logger,
            subs: vec![],
            frames: 0,
            cycles: 0,
            line_cycles: 0,
            frame_cycles: 0,
            frame_syncs: vec![],
            current_sub: None,
            current_subinfo: None,
            curr_frame: None,
        });
        s.calc();
        s
    }

    pub fn new_logger(&self) -> slog::Logger {
        // FIXME: add context logging current PC.
        // It's currently impossible with slog because it requires
        // context to be Send+Sync, which Subsytem is not.
        let sync2: *const Sync = &*self;
        let sync3: *const Sync = &*self;
        self.logger.new(o!("pc" => slog::FnValue(move |_| {
            let sync2 = unsafe { &*sync2 };
            sync2.current_pc().map_or("[none]".to_owned(), |pc| {
                if (pc as u32).sx64() == pc {
                    (pc as u32).hex()
                } else {
                    pc.hex()
                }
            })
        }),
        "sub" => slog::FnValue(move |_| {
            let sync3 = unsafe { &*sync3 };
            sync3.current_subinfo.as_ref().map_or("[none]", |i| &i.name)
        }),
        ))
    }

    pub fn register(&mut self, name: &str, sub: SubPtr, freq: i64) {
        self.subs.push(SubInfo {
            name: name.to_owned(),
            sub: sub,
            scaler: self.cfg.main_clock as f64 / freq as f64,
        });
    }

    fn calc(&mut self) {
        self.line_cycles = self.cfg.dot_clock_divider * self.cfg.hdots as i64;
        self.frame_cycles = self.line_cycles * self.cfg.vdots as i64;

        self.frame_syncs = self
            .cfg
            .vsyncs
            .iter()
            .map(|y| (self.line_cycles * *y as i64, Event::VSync(0, *y)))
            .collect::<Vec<_>>();
        for y in 0..self.cfg.vdots {
            let cycles = self.line_cycles * y as i64;
            let dot_clock_divider = self.cfg.dot_clock_divider;
            self.frame_syncs.extend(self.cfg.hsyncs.iter().map(|x| {
                (
                    cycles + *x as i64 * dot_clock_divider as i64,
                    Event::HSync(*x, y),
                )
            }))
        }
        // Use stable sort to make sure that VSyncs events are
        // generated before HSyncs events (on first pixel)
        self.frame_syncs.sort_by_key(|k| k.0);
    }

    pub fn current_pc(&self) -> Option<u64> {
        self.current_sub.map_or(None, |sub| unsafe { &*sub }.pc())
    }

    pub fn cycles(&self) -> i64 {
        let scaler: f64 = self.current_subinfo.as_ref().map_or(0.0, |i| i.scaler);
        match self.current_sub {
            Some(sub) => (unsafe { &*sub }.cycles() as f64 * scaler) as i64,
            None => self.cycles,
        }
    }

    // Return the (x,y) dot position of the emulation in the current frame.
    pub fn dot_pos(&self) -> (usize, usize) {
        let clk = self.cycles();
        let y = clk / self.line_cycles as i64;
        let x = clk % self.line_cycles as i64;
        (x as usize, y as usize)
    }

    fn do_frame<F: FnMut(Event)>(
        &mut self,
        mut cb: F,
        tracer: Option<&dyn dbg::Tracer>,
    ) -> dbg::Result<()> {
        let (frame_start, idx) = self.curr_frame.unwrap_or((self.cycles, 0));
        let frame_end = frame_start + self.frame_cycles;
        assert_eq!(frame_start % self.frame_cycles, 0);

        for idx in idx..self.frame_syncs.len() {
            self.curr_frame = Some((frame_start, idx));
            let (cyc, evt) = self.frame_syncs[idx];
            self.run_until(frame_start + cyc, tracer)?;
            cb(evt);
            if let Some(t) = tracer {
                match evt {
                    // FIXME: this relies on the fact that this specific HSync event
                    // was requested. Find out how to handle more generally.
                    Event::HSync(x, y) if x == 0 => t.trace_gpu(y)?,
                    _ => {}
                };
            }
        }

        self.curr_frame = Some((frame_start, self.frame_syncs.len()));
        self.run_until(frame_end, tracer)?;
        self.frames = self.frames + 1;
        self.curr_frame = None;
        Ok(())
    }

    pub fn trace_frame<F: FnMut(Event), T: dbg::Tracer>(
        &mut self,
        cb: F,
        tracer: &T,
    ) -> dbg::Result<()> {
        self.do_frame(cb, Some(tracer))
    }

    pub fn run_frame<F: FnMut(Event)>(&mut self, cb: F) {
        self.do_frame(cb, None).unwrap();
    }

    fn run_until(&mut self, target: i64, tracer: Option<&dyn dbg::Tracer>) -> dbg::Result<()> {
        for info in &self.subs {
            self.current_subinfo = Some(info.clone());
            let mut sub = info.sub.borrow_mut();
            self.current_sub = Some(&*sub);
            sub.run((target as f64 / info.scaler) as i64, tracer);
            self.current_sub = None;
            self.current_subinfo = None;
        }
        self.cycles = target;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate slog;
    use super::*;

    fn logger() -> slog::Logger {
        use slog::Drain;
        let decorator = slog_term::PlainSyncDecorator::new(std::io::stdout());
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        slog::Logger::root(drain, o!())
    }

    #[test]
    fn events() {
        let mut sync = Sync::new(
            logger(),
            Config {
                main_clock: 128,
                dot_clock_divider: 2,
                hdots: 4,
                vdots: 4,
                hsyncs: vec![0, 2],
                vsyncs: vec![2],
            },
        );

        let events = vec![
            (0, Event::HSync(0, 0)),
            (4, Event::HSync(2, 0)),
            (8, Event::HSync(0, 1)),
            (12, Event::HSync(2, 1)),
            (16, Event::VSync(0, 2)),
            (16, Event::HSync(0, 2)),
            (20, Event::HSync(2, 2)),
            (24, Event::HSync(0, 3)),
            (28, Event::HSync(2, 3)),
        ];
        assert_eq!(sync.frame_syncs, events);

        let mut record = Vec::new();
        sync.run_frame(|evt| {
            record.push(evt);
        });

        assert_eq!(
            record,
            events.iter().map(|(_, evt)| *evt).collect::<Vec<_>>()
        );
    }
}
