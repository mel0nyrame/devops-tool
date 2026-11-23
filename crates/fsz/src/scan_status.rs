use std::io::Write;
use std::time::{Duration, Instant};

const UPDATE_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) struct ScanStatus<W: Write> {
    writer: W,
    entries: u64,
    last_update: Instant,
    displayed_width: usize,
}

impl<W: Write> ScanStatus<W> {
    pub(crate) fn new(writer: W) -> Self {
        Self {
            writer,
            entries: 0,
            last_update: Instant::now(),
            displayed_width: 0,
        }
    }

    pub(crate) fn visit(&mut self) {
        self.visit_at(Instant::now());
    }

    fn visit_at(&mut self, now: Instant) {
        self.entries += 1;
        if now.duration_since(self.last_update) < UPDATE_INTERVAL {
            return;
        }

        // The line is ASCII and can only grow as the count increases.
        let line = format!("\rScanning... {} entries visited", self.entries);
        self.displayed_width = line.len() - 1;
        self.last_update = now;
        let _ = self.writer.write_all(line.as_bytes());
        let _ = self.writer.flush();
    }
}

impl<W: Write> Drop for ScanStatus<W> {
    fn drop(&mut self) {
        if self.displayed_width > 0 {
            let _ = write!(
                self.writer,
                "\r{:width$}\r",
                "",
                width = self.displayed_width
            );
            let _ = self.writer.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_scan_writes_nothing() {
        let mut output = Vec::new();
        {
            let mut status = ScanStatus::new(&mut output);
            let start = status.last_update;
            status.visit_at(start + UPDATE_INTERVAL - Duration::from_millis(1));
        }
        assert!(output.is_empty());
    }

    #[test]
    fn delays_throttles_and_clears_status() {
        let mut output = Vec::new();
        {
            let mut status = ScanStatus::new(&mut output);
            let start = status.last_update;
            status.visit_at(start);
            status.visit_at(start + UPDATE_INTERVAL);
            status.visit_at(start + UPDATE_INTERVAL + Duration::from_millis(1));
            status.visit_at(start + UPDATE_INTERVAL * 2);
        }
        let first = "Scanning... 2 entries visited";
        let second = "Scanning... 4 entries visited";
        assert_eq!(
            output,
            format!("\r{first}\r{second}\r{}\r", " ".repeat(second.len())).as_bytes()
        );
    }
}
