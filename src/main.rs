#[macro_use]
extern crate error_chain;

use emu::dbg;
use emu::hw;
use emu::log;
use r64emu::errors::*;
use r64emu::N64;

use std::path::Path;

use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
struct Cli {
    /// Activate debugger at start
    #[structopt(short = "d", long = "debugger")]
    debugger: bool,

    /// Path to the BIOS file
    #[structopt(
        short = "b",
        long = "bios",
        parse(from_os_str),
        default_value = "bios/pifdata.bin"
    )]
    bios: std::path::PathBuf,

    /// Path to the ROM file
    #[structopt(parse(from_os_str))]
    rom: std::path::PathBuf,
}

quick_main!(run);

fn create_n64(romfn: &Path, biosfn: &Path, logger: slog::Logger) -> Result<N64> {
    let mut n64 = N64::new(logger, romfn, biosfn).unwrap();
    n64.setup_cic(true)?;
    Ok(n64)
}

fn run() -> Result<()> {
    let args = Cli::from_args();

    let mut out = hw::Output::new(
        hw::VideoConfig {
            window_title: "R64EMU - Nintendo 64 Emulator".into(),
            width: 640,
            height: 480,
            fps: 60,
        },
        hw::AudioConfig {
            frequency: N64::AUDIO_OUTPUT_FREQUENCY as isize,
        },
    )?;
    out.enable_video()?;
    out.enable_audio()?;

    if args.debugger {
        let (logger, logpool) = dbg::new_debugger_logger();
        let mut n64 = create_n64(&args.rom, &args.bios, logger).unwrap();
        let mut dbgconfig = args.rom.clone();
        dbgconfig.set_extension("dbg");
        out.run_and_debug(&mut n64, &dbgconfig, logpool);
    } else {
        out.run_threaded(move || {
            let logger = log::new_console_logger();
            let n64 = create_n64(&args.rom, &args.bios, logger).unwrap();
            Ok(Box::new(n64))
        });
    }

    Ok(())
}
