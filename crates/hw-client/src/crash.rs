//! The crash reporter. A panic anywhere, or Engine giving up, writes a
//! report (version, what was going on, the message, where, and the stack)
//! into `crashes/` beside the profile. The next start tells the player on
//! the title where it is, so it can be sent to Arnd.

use std::path::PathBuf;
use std::sync::Mutex;

/// What the game is doing, for the report: (what, value), newest last.
static CONTEXT: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Notes something the report should say (the world seed, hosting,
/// joined players...). The same `what` again replaces the old value.
pub fn note(what: &str, value: impl ToString) {
    if let Ok(mut c) = CONTEXT.lock() {
        let value = value.to_string();
        match c.iter_mut().find(|(k, _)| k == what) {
            Some(slot) => slot.1 = value,
            None => c.push((what.to_string(), value)),
        }
    }
}

fn folder() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("HW_CRASH_DIR") {
        return Some(PathBuf::from(d));
    }
    crate::profile::Profile::dir().map(|d| d.join("crashes"))
}

/// The report's text.
pub fn report(what: &str, message: &str, at: Option<String>, stack: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("HeroWorld {} crash report\n", hw_core::version()));
    out.push_str(&format!(
        "System: {} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    out.push_str(&format!("When: {secs} (seconds since 1970, UTC)\n"));
    out.push_str(&format!("What: {what}\n"));
    out.push_str(&format!("Message: {message}\n"));
    if let Some(at) = at {
        out.push_str(&format!("Where: {at}\n"));
    }
    if let Ok(c) = CONTEXT.lock() {
        if !c.is_empty() {
            out.push_str("\nGoing on:\n");
            for (k, v) in c.iter() {
                out.push_str(&format!("  {k}: {v}\n"));
            }
        }
    }
    out.push_str("\nStack:\n");
    out.push_str(stack);
    out.push('\n');
    out
}

/// Writes a report; `fatal` ones (the game closed) are announced on the
/// next start. Returns where it went.
pub fn write(text: &str, fatal: bool) -> Option<PathBuf> {
    let dir = folder()?;
    std::fs::create_dir_all(&dir).ok()?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let kind = if fatal { "crash" } else { "panic" };
    let path = dir.join(format!("{kind}-{secs}.txt"));
    std::fs::write(&path, text).ok()?;
    if fatal {
        let _ = std::fs::write(dir.join("unseen"), path.to_string_lossy().as_bytes());
    }
    Some(path)
}

/// Catches panics from now on. A panic on the main thread closes the game
/// (a fatal report); one on a worker thread is written down and the game
/// goes on as well as it can.
pub fn install() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        let at = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed").to_string();
        let stack = std::backtrace::Backtrace::force_capture().to_string();
        let fatal = name == "main";
        let text = report(&format!("panic on thread '{name}'"), &message, at, &stack);
        if let Some(p) = write(&text, fatal) {
            eprintln!("A crash report was written to {}", p.display());
        }
        before(info);
    }));
}

/// Engine stopped with an error: a fatal report.
pub fn engine_error(e: &dyn std::fmt::Display) {
    let stack = std::backtrace::Backtrace::force_capture().to_string();
    let text = report("Engine stopped with an error", &e.to_string(), None, &stack);
    if let Some(p) = write(&text, true) {
        eprintln!("A crash report was written to {}", p.display());
    }
}

/// The report of the last time the game closed unexpectedly, once: the
/// next call says nothing until the next crash.
pub fn take_unseen() -> Option<PathBuf> {
    let marker = folder()?.join("unseen");
    let path = std::fs::read_to_string(&marker).ok()?;
    let _ = std::fs::remove_file(&marker);
    let path = PathBuf::from(path.trim());
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_is_written_and_announced_once() {
        let dir = std::env::temp_dir().join(format!("hw-crash-{}", std::process::id()));
        std::env::set_var("HW_CRASH_DIR", &dir);
        note("world seed", 42);
        note("world seed", 43);
        let text = report("test", "boom", Some("here.rs:1:2".into()), "stack");
        assert!(text.contains("Message: boom"));
        assert!(text.contains("world seed: 43") && !text.contains("world seed: 42"));
        let p = write(&text, true).expect("written");
        assert_eq!(take_unseen(), Some(p.clone()));
        assert_eq!(take_unseen(), None);
        // A worker's panic is written but not announced.
        write(&text, false).expect("written");
        assert_eq!(take_unseen(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
