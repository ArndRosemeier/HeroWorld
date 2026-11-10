//! The debug log: what happened to key presses and powers, in words, from
//! the keyboard through the client to the server and back. The debug menu
//! shows it (the Log tab, with a Copy button), and every line also goes to
//! `logs/debug.log` beside start.bat (written on a thread of its own).

use std::collections::VecDeque;
use std::io::Write as _;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Lines kept for the menu.
const KEEP: usize = 600;
/// The file starts over past this size.
const FILE_LIMIT: u64 = 4 << 20;

struct Log {
    started: Instant,
    lines: VecDeque<String>,
    file: Option<Sender<String>>,
}

fn log() -> &'static Mutex<Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(|| {
        Mutex::new(Log {
            started: Instant::now(),
            lines: VecDeque::new(),
            file: writer(),
        })
    })
}

/// A thread that appends lines to `logs/debug.log`.
fn writer() -> Option<Sender<String>> {
    let (tx, rx) = channel::<String>();
    std::thread::Builder::new()
        .name("hw-debug-log".into())
        .spawn(move || {
            let dir = std::path::PathBuf::from("logs");
            let dir = if std::fs::create_dir_all(&dir).is_ok() {
                dir
            } else {
                match crate::profile::Profile::dir() {
                    Some(d) => d.join("logs"),
                    None => return,
                }
            };
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join("debug.log");
            let open = |fresh: bool| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(!fresh)
                    .truncate(fresh)
                    .open(&path)
                    .ok()
            };
            // A new game starts a new file.
            let mut file = open(true);
            for line in rx {
                if std::fs::metadata(&path).is_ok_and(|m| m.len() > FILE_LIMIT) {
                    file = open(true);
                }
                if let Some(f) = file.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
            }
        })
        .ok()?;
    Some(tx)
}

/// Adds a line (the seconds since the game started go in front).
pub fn add(text: impl AsRef<str>) {
    let Ok(mut l) = log().lock() else {
        return;
    };
    let line = format!(
        "{:8.2}  {}",
        l.started.elapsed().as_secs_f32(),
        text.as_ref()
    );
    if let Some(f) = &l.file {
        let _ = f.send(line.clone());
    }
    l.lines.push_back(line);
    while l.lines.len() > KEEP {
        l.lines.pop_front();
    }
}

/// The lines kept, oldest first.
pub fn lines() -> Vec<String> {
    log()
        .lock()
        .map(|l| l.lines.iter().cloned().collect())
        .unwrap_or_default()
}

/// Forgets the lines kept (the file keeps them).
pub fn clear() {
    if let Ok(mut l) = log().lock() {
        l.lines.clear();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lines_are_kept_in_order_and_bounded() {
        super::clear();
        for k in 0..super::KEEP + 10 {
            super::add(format!("line {k}"));
        }
        let lines = super::lines();
        assert!(lines.len() <= super::KEEP);
        assert!(lines
            .last()
            .unwrap()
            .ends_with(&format!("line {}", super::KEEP + 9)));
    }
}
