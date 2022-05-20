use emu::bus::be::{Bus, Device};
use emu::dbg;
use emu::dbg::{DebuggerModel, DebuggerRenderer};
use emu::gfx::{GfxBufferMutLE, Rgb888};
use emu::hw;
use emu::input::*;
use emu::snd::{SampleFormat, SndBufferMut, S16_STEREO};
use emu::state::{CurrentState, State};
use emu::sync;
use emu::sync::Subsystem;
use emu_derive::DeviceBE;

use slog;
use std::ops::{Deref, DerefMut};
use std::path::Path;

use super::ai::Ai;
use super::cartridge::{Cartridge, CicModel};
use super::dp::Dp;
use super::errors::*;
use super::mi::Mi;
use super::mips64;
use super::pi::Pi;
use super::r4300::R4300;
use super::ri::Ri;
use super::si::Si;
use super::sp::{Sp, RSPCPU};
use super::vi::Vi;

// Used in debugger windows
pub(crate) const MAINCPU_NAME: &'static str = "R4300";
pub(crate) const RSPCPU_NAME: &'static str = "RSP";

pub struct N64 {
    logger: slog::Logger,
    sync: Box<sync::Sync<SyncEmu>>,
    initial_state: State,
}

// N64 timings
// https://assemblergames.com/threads/mapping-n64-overclockability-achieved-3-0x-multiplier-but-not-3-0x-speed.51656/

// Oscillators
const X1: i64 = 14_705_000;
const X2: i64 = 14_318_000;

const RDRAM_CLOCK: i64 = X1 * 17;
const MAIN_CLOCK: i64 = RDRAM_CLOCK / 4;
const _PIF_CLOCK: i64 = MAIN_CLOCK / 4;
const _CARTRIDGE_CLOCK: i64 = _PIF_CLOCK / 8; // 1.953 MHZ
pub(crate) const VCLK: i64 = X2 * 17 / 5; // 48.6812 MHZ

struct SyncEmu;
impl sync::SyncEmu for SyncEmu {
    fn config(&self) -> sync::Config {
        sync::Config {
            main_clock: VCLK,
            dot_clock_divider: 2,
            hdots: 773, // 773.5...
            vdots: 525,
            hsyncs: vec![0, 773 / 2], // sync two times per line
            vsyncs: vec![],
        }
    }
    fn subsystem(&self, idx: usize) -> Option<(&mut dyn sync::Subsystem, i64)> {
        match idx {
            0 => Some((R4300::get_mut().deref_mut(), MAIN_CLOCK + MAIN_CLOCK / 2)), // FIXME: uses DIVMOD),
            1 => Some((RSPCPU::get_mut().deref_mut(), MAIN_CLOCK)),
            2 => Some((Dp::get_mut(), MAIN_CLOCK)),
            3 => Some((Ai::get_mut(), VCLK)),
            4 => Some((Pi::get_mut(), MAIN_CLOCK)),
            _ => None,
        }
    }
}

pub(crate) const JOY_NAMES: [&'static str; 4] = ["joy1", "joy2", "joy3", "joy4"];

fn create_input_manager() -> InputManager {
    let joy = InputDevice::new(
        "joy-template",
        InputDeviceKind::Joystick,
        vec![
            Input::new_digital("up", InputKind::Up, 27),
            Input::new_digital("down", InputKind::Down, 26),
            Input::new_digital("left", InputKind::Left, 25),
            Input::new_digital("right", InputKind::Right, 24),
            Input::new_digital("A", InputKind::Button1, 31),
            Input::new_digital("B", InputKind::Button2, 30),
            Input::new_digital("Z", InputKind::Button3, 29),
            Input::new_digital("S", InputKind::Start, 28),
            Input::new_digital("c-up", InputKind::Up, 19),
            Input::new_digital("c-down", InputKind::Down, 18),
            Input::new_digital("c-left", InputKind::Left, 17),
            Input::new_digital("c-right", InputKind::Right, 16),
            Input::new_digital("L", InputKind::Other, 21),
            Input::new_digital("R", InputKind::Other, 20),
            Input::new_analog("X", InputKind::Horizontal, 8),
            Input::new_analog("Y", InputKind::Vertical, 0),
        ],
    );

    InputManager::new(vec![
        joy.dup(JOY_NAMES[0]),
        joy.dup(JOY_NAMES[1]),
        joy.dup(JOY_NAMES[2]),
        joy.dup(JOY_NAMES[3]),
        InputDevice::new(
            "console",
            InputDeviceKind::Other,
            vec![Input::new_digital("reset", InputKind::Other, 0)],
        ),
    ])
}

impl N64 {
    pub const AUDIO_OUTPUT_FREQUENCY: i64 = Ai::OUTPUT_FREQUENCY;

    pub fn new(logger: slog::Logger, romfn: &Path, biosfn: &Path) -> Result<N64> {
        let sync = sync::Sync::new(logger.new(o!()), SyncEmu);

        R4300::new(sync::Sync::new_logger(&sync)).register();
        Mi::new(sync::Sync::new_logger(&sync)).register();
        Cartridge::new(sync::Sync::new_logger(&sync), romfn)
            .chain_err(|| "cannot open rom file")?
            .register();

        Pi::new(
            sync::Sync::new_logger(&sync),
            biosfn,
            create_input_manager(),
        )
        .chain_err(|| "cannot open BIOS file")?
        .register();
        Dp::new(sync::Sync::new_logger(&sync)).register();
        Sp::new(sync::Sync::new_logger(&sync))?.register();
        Si::new(sync::Sync::new_logger(&sync)).register();
        Vi::new(sync::Sync::new_logger(&sync)).register();
        Ai::new(sync::Sync::new_logger(&sync)).register();
        Ri::new(sync::Sync::new_logger(&sync)).register();

        // Now that all devices have been created, map the CPU buses.
        R4300::get_mut().map_bus()?;
        RSPCPU::get_mut().map_bus()?;

        return Ok(N64 {
            logger,
            sync,
            initial_state: CurrentState().clone(),
        });
    }

    // Setup the CIC (copy protection) emulation.
    pub fn setup_cic(&mut self, hard_reset: bool) -> Result<()> {
        // The 32-bit word at offset 0x24 in PIF RAM (bus addr: 0x1FC0_07E4)
        // is filled by PIF during boot. It contains the encryption seed
        // (that PIF got after negotiation with CIC), and some other information
        // that the CPU can use.

        // bits     | reg | description
        // 00080000 | S3  | osRomType (0=GamePack, 1=DD)
        // 00040000 | S7  | osVersion
        // 00020000 | S5  | osResetType (1 = NMI, 0 = cold reset)
        // 0000FF00 | S6  | CIC IPL3 seed value
        // 000000FF | --  | CIC IPL2 seed value
        // -------- | S4  | TV Type (0=PAL, 1=NTSC, 2=MPAL)

        // Setup the encryption seed, given the CIC model that we detect
        // by checksumming the ROM header.
        let mut seed: u32 = match Cartridge::get().detect_cic_model()? {
            CicModel::Cic6101 => 0x3F, // starfox
            CicModel::Cic6102 => 0x3F, // mario
            CicModel::Cic6103 => 0x78, // banjo
            CicModel::Cic6105 => 0x91, // zelda
            CicModel::Cic6106 => 0x85, // f-zero x
        } << 8;

        // Set the NMI/reset bit
        if !hard_reset {
            seed |= 0x0002_0000;
        }

        R4300::get_mut().bus.write::<u32>(0x1FC0_07E4, seed);

        // FIXME: fix RDRAM initialization emulation. IPL3 does initialize RDRAM (starting at 0x0400_0040),
        // and is supposed to end up writing the RAM size at 0x8000_0318, but it does not currently work.
        // This is relied upon by libdragon at least. So fix it by setting the RDRAM as already initialized
        // and copying the RAM size.
        R4300::get_mut().bus.write::<u32>(0x0470_000C, 0x14);
        R4300::get_mut()
            .bus
            .write::<u32>(0x0000_0318, 4 * 1024 * 1024);
        Ok(())
    }
}

impl hw::OutputProducer for N64 {
    type AudioSampleFormat = S16_STEREO;

    fn render_frame(
        &mut self,
        screen: &mut GfxBufferMutLE<Rgb888>,
        sound: &mut SndBufferMut<Self::AudioSampleFormat>,
    ) {
        self.sync.run_frame(|evt| match evt {
            sync::Event::BeginFrame => {
                Vi::get_mut().begin_frame(screen);
                Ai::get_mut().begin_frame(sound);
                Pi::get_mut().begin_frame();
            }
            sync::Event::HSync(x, y) if x == 0 => {
                Vi::get_mut().set_line(y);
            }
            sync::Event::EndFrame => {
                Vi::get_mut().end_frame(screen);
                Ai::get_mut().end_frame(sound);
                Pi::get_mut().end_frame();
            }
            _ => {}
        });
    }

    fn input_manager(&mut self) -> Option<&mut InputManager> {
        Some(&mut Pi::get_mut().input)
    }
}

impl DebuggerModel for N64 {
    fn trace_frame<SF: SampleFormat>(
        &mut self,
        screen: &mut GfxBufferMutLE<Rgb888>,
        sound: &mut SndBufferMut<SF>,
        tracer: &dbg::Tracer,
    ) -> dbg::Result<()> {
        self.sync.trace_frame(
            |evt| match evt {
                sync::Event::BeginFrame => {
                    Vi::get_mut().begin_frame(screen);
                    Ai::get_mut().begin_frame(sound);
                    Pi::get_mut().begin_frame();
                }
                sync::Event::EndFrame => {
                    Vi::get_mut().end_frame(screen);
                    Ai::get_mut().end_frame(sound);
                    Pi::get_mut().end_frame();
                }
                sync::Event::HSync(x, y) if x == 0 => {
                    Vi::get_mut().set_line(y);
                }
                _ => {}
            },
            tracer,
        )?;
        Ok(())
    }

    fn trace_step(&mut self, cpu_name: &str, tracer: &dbg::Tracer) -> dbg::Result<()> {
        match cpu_name {
            MAINCPU_NAME => R4300::get_mut().step(tracer),
            RSPCPU_NAME => RSPCPU::get_mut().step(tracer),
            _ => unreachable!(),
        }
    }

    fn render_debug<'a, 'ui>(&mut self, dr: &DebuggerRenderer<'a, 'ui>) {
        R4300::get_mut().render_debug(dr);
        RSPCPU::get_mut().render_debug(dr);
    }

    fn all_cpus(&self) -> Vec<String> {
        vec![MAINCPU_NAME.into(), RSPCPU_NAME.into()]
    }

    fn cycles(&self) -> i64 {
        self.sync.cycles()
    }

    fn frames(&self) -> i64 {
        self.sync.frames()
    }

    fn reset(&mut self, hard: bool) {
        if hard {
            // Hard reset: restore initial emulator status
            self.initial_state.clone().make_current();
            self.setup_cic(true).unwrap();
            self.sync.reset();
        } else {
            // Soft reset: just trigger a reset on CPUs and hope for the best
            R4300::get_mut().reset();
            RSPCPU::get_mut().reset();
            self.setup_cic(false).unwrap();
        }
    }
}
