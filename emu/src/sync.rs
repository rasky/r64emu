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
    // Run the subsytem until the specified target of cycles is reached
    fn run(&mut self, target_cycles: i64);

    // Return the current number of cycles elapsed in the subsytem.
    // Notice that this might be called from within run(), and it's supposed
    // to always hold the updated value
    fn cycles(&self) -> i64;
}

type SubPtr = Rc<RefCell<Subsystem>>;

pub struct Sync {
    pub cfg: Config,
    subs: Vec<SubPtr>,
    sub_scaler: Vec<f64>,
    current_sub: Option<*const Subsystem>,

    frames: i64,
    cycles: i64,
    line_cycles: i64,
    frame_cycles: i64,
    frame_syncs: Vec<(i64, Event)>,
}

impl Sync {
    pub fn new(cfg: Config) -> Sync {
        let mut s = Sync {
            cfg,
            subs: vec![],
            sub_scaler: vec![],
            frames: 0,
            cycles: 0,
            line_cycles: 0,
            frame_cycles: 0,
            frame_syncs: vec![],
            current_sub: None,
        };
        s.calc();
        s
    }

    pub fn register(&mut self, sub: SubPtr, freq: i64) {
        self.subs.push(sub);
        self.sub_scaler
            .push(self.cfg.main_clock as f64 / freq as f64);
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

    pub fn cycles(&self) -> i64 {
        match self.current_sub {
            Some(sub) => unsafe { &*sub }.cycles(),
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

    pub fn run_frame<F: FnMut(Event)>(&mut self, mut cb: F) {
        let frame_start = self.cycles;
        let frame_end = frame_start + self.frame_cycles;
        assert_eq!(frame_start % self.frame_cycles, 0);

        for idx in 0..self.frame_syncs.len() {
            let (cyc, evt) = self.frame_syncs[idx];
            self.run_until(frame_start + cyc);
            cb(evt);
        }

        self.run_until(frame_end);
        self.frames = self.frames + 1;
    }

    fn run_until(&mut self, target: i64) {
        for (sub, scaler) in self.subs.iter().zip(self.sub_scaler.iter()) {
            let mut sub = sub.borrow_mut();
            self.current_sub = Some(&*sub);
            sub.run((target as f64 / scaler) as i64);
            self.current_sub = None;
        }
        self.cycles = target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events() {
        let mut sync = Sync::new(Config {
            main_clock: 128,
            dot_clock_divider: 2,
            hdots: 4,
            vdots: 4,
            hsyncs: vec![0, 2],
            vsyncs: vec![2],
        });

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
