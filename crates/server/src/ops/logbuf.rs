//! The server's recent log lines, kept in memory for the support bundle (support.rs,
//! docs/support.md): the same lines as stdout, without colour, the newest `MAX_LINES`. Gone at
//! a restart; the container's own log (`docker logs roastwarden`) keeps more.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Mutex;

pub const MAX_LINES: usize = 2000;
/// A longer line is cut (a dumped payload)
const MAX_LINE: usize = 2000;

static LINES: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

fn push(text: &str) {
    let mut lines = LINES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut l = line.to_string();
        if l.len() > MAX_LINE {
            let mut cut = MAX_LINE;
            while !l.is_char_boundary(cut) {
                cut -= 1;
            }
            l.truncate(cut);
            l.push_str(" …");
        }
        if lines.len() == MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(l);
    }
}

/// The lines kept, oldest first.
pub fn lines() -> Vec<String> {
    LINES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .cloned()
        .collect()
}

/// A `tracing_subscriber` writer: each event's formatted line goes into the ring.
#[derive(Clone, Copy, Default)]
pub struct Ring;

pub struct RingWriter(Vec<u8>);

impl Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for RingWriter {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            push(&String::from_utf8_lossy(&self.0));
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Ring {
    type Writer = RingWriter;
    fn make_writer(&'a self) -> Self::Writer {
        RingWriter(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::fmt::MakeWriter;

    #[test]
    fn keeps_the_newest_lines_cut_to_length() {
        // The ring is shared by the process: look at what this test pushed only
        push("first line\nsecond line\n");
        let long = "x".repeat(MAX_LINE + 50);
        {
            let mut w = Ring.make_writer();
            w.write_all(long.as_bytes()).unwrap();
        }
        let all = lines();
        let at = all.iter().rposition(|l| l == "first line").unwrap();
        assert_eq!(all[at + 1], "second line");
        let cut = all.iter().rfind(|l| l.starts_with("xxx")).unwrap();
        assert!(
            cut.ends_with(" …") && cut.len() < long.len(),
            "{}",
            cut.len()
        );
        for i in 0..MAX_LINES + 10 {
            push(&format!("n{i}"));
        }
        let all = lines();
        assert_eq!(all.len(), MAX_LINES);
        assert_eq!(all.last().unwrap(), &format!("n{}", MAX_LINES + 9));
    }
}
