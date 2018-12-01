#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate slog;
extern crate slog_term;

extern crate emu;
extern crate r64emu;

use emu::hw;
use r64emu::errors::*;
use r64emu::N64;
use slog::Drain;
use std::env;

fn module_and_line(record: &slog::Record) -> String {
    format!("{}:{}", record.module(), record.line())
}

#[allow(dead_code)]
fn log_build_sync() -> slog::Logger {
    let decorator = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    slog::Logger::root(drain, o!("module" => slog::FnValue(module_and_line)))
}

quick_main!(run);

fn create_n64(romfn: &str) -> Result<N64> {
    let logger = log_build_sync();
    let n64 = N64::new(logger, romfn).unwrap();
    n64.setup_cic().unwrap();
    Ok(n64)
}

fn run() -> Result<()> {
    let logger = log_build_sync();
    crit!(logger, "Hello World!");

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        bail!("Usage: r64emu [rom]");
    }

    let mut out = hw::Output::new(hw::OutputConfig {
        window_title: "R64EMU - Nintendo 64 Emulator".into(),
        width: 640,
        height: 480,
        fps: 60,
        enforce_speed: false,
    })?;
    out.enable_video()?;

    let romfn = args[1].clone();

    if true {
        let mut n64 = create_n64(&romfn).unwrap();
        out.run_and_debug(&mut n64);
    } else {
        out.run_threaded(move || Ok(Box::new(create_n64(&romfn).unwrap())));
    }

    Ok(())
}
