use once_cell::sync::OnceCell;
use std::sync::{Once, RwLock as StdRwLock};
use tokio::sync::broadcast;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;

// Keep the non-blocking worker guard alive for the entire process lifetime
pub static LOG_GUARD: OnceCell<WorkerGuard> = OnceCell::new();
pub static INIT_ONCE: Once = Once::new();
pub static INIT_ERROR: OnceCell<String> = OnceCell::new();
pub static LOG_BROADCAST_TX: OnceCell<broadcast::Sender<String>> = OnceCell::new();
pub static WEB_LOG_LEVEL: OnceCell<StdRwLock<Level>> = OnceCell::new();

pub fn set_web_log_level(new_level: Level) {
    if let Some(lock) = WEB_LOG_LEVEL.get() {
        if let Ok(mut guard) = lock.write() {
            *guard = new_level;
        }
    } else {
        let _ = WEB_LOG_LEVEL.set(StdRwLock::new(new_level));
    }
}

pub fn get_web_log_level() -> Level {
    if let Some(lock) = WEB_LOG_LEVEL.get() {
        if let Ok(guard) = lock.read() {
            *guard
        } else {
            Level::INFO
        }
    } else {
        Level::INFO
    }
}
