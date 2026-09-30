use std::{
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

use log::{Level, LevelFilter, Log, Metadata, Record};

struct RooseLogger;

impl Log for RooseLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info || (metadata.level() == Level::Debug && debug_enabled())
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(
            std::io::stdout(),
            "{elapsed} {} {}",
            record.level(),
            record.args()
        );
    }

    fn flush(&self) {}
}

fn debug_enabled() -> bool {
    std::env::var("RUST_LOG")
        .map(|value| {
            value
                .split(',')
                .any(|part| part.ends_with("=debug") || part == "debug")
        })
        .unwrap_or(false)
}

pub fn init() {
    static LOGGER: RooseLogger = RooseLogger;
    if let Err(error) = log::set_logger(&LOGGER) {
        eprintln!("roose: couldn't initialize logging: {error}");
        return;
    }
    log::set_max_level(if debug_enabled() {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    });
}
