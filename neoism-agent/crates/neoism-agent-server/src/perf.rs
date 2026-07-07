use std::sync::OnceLock;
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();

pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("NEOISM_AGENT_PERF_LOG").is_some())
}

pub(crate) fn now() -> Option<Instant> {
    enabled().then(Instant::now)
}

pub(crate) fn elapsed_ms(started: Option<Instant>) -> Option<u128> {
    started.map(|started| started.elapsed().as_millis())
}
