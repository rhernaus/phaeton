use crate::logging::state::LOG_BROADCAST_TX;
use std::io::{self, Write};
use tokio::sync::broadcast;
use tracing_subscriber::fmt::writer::MakeWriter;

#[derive(Clone)]
pub struct BroadcastMakeWriter {
    pub(crate) tx: broadcast::Sender<String>,
}

pub struct BroadcastWriter {
    tx: broadcast::Sender<String>,
    buffer: Vec<u8>,
}

impl<'a> MakeWriter<'a> for BroadcastMakeWriter {
    type Writer = BroadcastWriter;
    fn make_writer(&'a self) -> Self::Writer {
        BroadcastWriter {
            tx: self.tx.clone(),
            buffer: Vec::with_capacity(256),
        }
    }
}

impl Write for BroadcastWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for BroadcastWriter {
    fn drop(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let mut line = String::from_utf8_lossy(&self.buffer).to_string();
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        let _ = self.tx.send(line);
    }
}

pub fn get_or_init_log_tx() -> broadcast::Sender<String> {
    LOG_BROADCAST_TX
        .get_or_init(|| {
            let (tx, _rx) = broadcast::channel::<String>(1024);
            tx
        })
        .clone()
}

pub fn subscribe_log_lines() -> broadcast::Receiver<String> {
    get_or_init_log_tx().subscribe()
}
