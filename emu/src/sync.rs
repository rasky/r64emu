use slog::*;

use crate::dbg;
use crate::int::Numerics;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    BeginFrame,
    EndFrame,
    HSync(usize, usize),
    VSync(usize, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub main_clock: i64,
    pub dot_clock_divider: i64,
    pub hdots: usize,
    pub vdots: usize,
    pub hsyncs: Vec<usize>,
    pub vsyncs: Vec<usize>,
}

pub trait Subsystem {
    /// Return the name of the subsystem (used for debugging).
    fn name(&self) -> &str;

    // Run the subsytem until the specified target of cycles is reached.
    // Optionally, report events to the specified tracer (debugger).
    fn run(&mut self, target_cycles: i64, tracer: &dbg::Tracer) -> dbg::Result<()>;

    // Do a single CPU step.
    fn step(&mut self, tracer: &dbg::Tracer) -> dbg::Result<()>;

    // Return the current number of cycles elapsed in the subsytem.
    // Notice that this might be called from within run(), and it's supposed
    // to always hold the updated value
    fn cycles(&self) -> i64;

    // Return the program counter for this subsystem (if any)
    fn pc(&self) -> Option<u64>;
}

pub trait SyncEmu {
    fn config(&self) -> Config;
    fn subsystem(&self, idx: usize) -> Option<(&mut dyn Subsystem, i64)>;
}

pub struct Sync<E: SyncEmu + 'static> {
    emu: E,
    cfg: Config,
    logger: slog::Logger,

    current_sub: Option<usize>,
    frames: i64,
    cycles: i64,
    line_cycles: i64,
    frame_cycles: i64,
    frame_syncs: Vec<(i64, Event)>,
    curr_frame: Option<(i64, usize)>,
}

impl<E: SyncEmu + 'static> Sync<E> {
    pub fn new(logger: slog::Logger, emu: E) -> Box<Self> {
        let mut s = Box::new(Self {
            cfg: emu.config(),
            emu,
            logger,
            frames: 0,
            cycles: 0,
            current_sub: None,
            line_cycles: 0,
            frame_cycles: 0,
            frame_syncs: vec![],
            curr_frame: None,
        });
        s.calc();
        s
    }

    pub fn new_logger(&self) -> slog::Logger {
        let sync2: *const Self = &*self;
        let sync3: *const Self = &*self;
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
            sync3.current_sub().map_or("[none]", |(s,_)| s.name())
        }),
        ))
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

    fn current_sub(&self) -> Option<(&mut dyn Subsystem, i64)> {
        self.current_sub.map(|idx| self.emu.subsystem(idx).unwrap())
    }

    pub fn current_pc(&self) -> Option<u64> {
        self.current_sub().map_or(None, |(s, _)| s.pc())
    }

    pub fn reset(&mut self) {
        self.frames = 0;
        self.cycles = 0;
        self.curr_frame = None;
    }

    pub fn frames(&self) -> i64 {
        self.frames
    }

    pub fn cycles(&self) -> i64 {
        match self.current_sub() {
            Some((sub, freq)) => {
                ((sub.cycles() as f64 * self.cfg.main_clock as f64) / freq as f64) as i64
            }
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

    fn do_frame<F: FnMut(Event)>(&mut self, mut cb: F, tracer: &dbg::Tracer) -> dbg::Result<()> {
        if self.curr_frame.is_none() {
            cb(Event::BeginFrame);
        }
        let (frame_start, idx) = self.curr_frame.unwrap_or((self.cycles, 0));
        let frame_end = frame_start + self.frame_cycles;
        assert_eq!(frame_start % self.frame_cycles, 0);

        for idx in idx..self.frame_syncs.len() {
            self.curr_frame = Some((frame_start, idx));
            let (cyc, evt) = self.frame_syncs[idx];
            self.run_until(frame_start + cyc, tracer)?;
            cb(evt);

            // Trace GPU lines.
            // FIXME: this relies on the fact that this specific HSync event
            // was requested. Find out how to handle more generally.
            match evt {
                Event::HSync(x, y) if x == 0 => tracer.trace_gpu(y)?,
                _ => {}
            };
        }

        self.curr_frame = Some((frame_start, self.frame_syncs.len()));
        self.run_until(frame_end, tracer)?;
        self.frames = self.frames + 1;
        self.curr_frame = None;
        cb(Event::EndFrame);
        Ok(())
    }

    pub fn trace_frame<F: FnMut(Event)>(&mut self, cb: F, tracer: &dbg::Tracer) -> dbg::Result<()> {
        self.do_frame(cb, tracer)
    }

    pub fn run_frame<F: FnMut(Event)>(&mut self, cb: F) {
        // When using a null tracer, do_frame() should only exit at the end of
        // the frame, as there's no reason to block execution.
        self.do_frame(cb, &dbg::Tracer::null()).unwrap();
    }

    fn run_until(&mut self, target: i64, tracer: &dbg::Tracer) -> dbg::Result<()> {
        let mut idx: usize = 0;
        while let Some((sub, freq)) = self.emu.subsystem(idx) {
            self.current_sub = Some(idx);
            let res = sub.run(
                (target as f64 * freq as f64 / self.cfg.main_clock as f64) as i64,
                tracer,
            );
            self.current_sub = None;
            res?;
            idx += 1;
        }
        self.cycles = target;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::new_console_logger;

    struct FakeEmu {
        cfg: Config,
    }

    impl SyncEmu for FakeEmu {
        fn config(&self) -> Config {
            self.cfg.clone()
        }
        fn subsystem(&self, _idx: usize) -> Option<(&mut dyn Subsystem, i64)> {
            None
        }
    }

    #[test]
    fn events() {
        let mut sync = Sync::new(
            new_console_logger(),
            FakeEmu {
                cfg: Config {
                    main_clock: 128,
                    dot_clock_divider: 2,
                    hdots: 4,
                    vdots: 4,
                    hsyncs: vec![0, 2],
                    vsyncs: vec![2],
                },
            },
        );

        let events = vec![
            (0, Event::BeginFrame),
            (0, Event::HSync(0, 0)),
            (4, Event::HSync(2, 0)),
            (8, Event::HSync(0, 1)),
            (12, Event::HSync(2, 1)),
            (16, Event::VSync(0, 2)),
            (16, Event::HSync(0, 2)),
            (20, Event::HSync(2, 2)),
            (24, Event::HSync(0, 3)),
            (28, Event::HSync(2, 3)),
            (28, Event::EndFrame),
        ];
        assert_eq!(&sync.frame_syncs[..], &events[1..events.len() - 1]);

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
