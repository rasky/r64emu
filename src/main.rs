#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate slog;
extern crate slog_async;
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

#[allow(dead_code)]
fn log_build_async() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!("module" => slog::FnValue(module_and_line)))
}

quick_main!(run);

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

    let logger1 = logger.clone();
    let romfn = args[1].clone();
    out.run(move || {
        let n64 = Box::new(N64::new(logger1, &romfn).unwrap());
        n64.setup_cic().unwrap();
        Ok(n64)
    });

    Ok(())
}
