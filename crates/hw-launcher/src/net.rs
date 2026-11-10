//! The release on GitHub, over HTTPS.

use crate::install::Source;
use crate::manifest::{sha256_file, Manifest};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// Files of the release at `base` (a URL ending in `/`).
pub struct GitHub {
    agent: ureq::Agent,
    base: String,
}

impl GitHub {
    pub fn new(base: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        GitHub {
            agent: ureq::Agent::new_with_config(config),
            base: base.to_string(),
        }
    }

    /// The newest release's manifest.
    pub fn manifest(&self) -> Result<Manifest, String> {
        let url = format!("{}manifest.json", self.base);
        let mut resp = self.agent.get(&url).call().map_err(|e| friendly(&url, e))?;
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("reading the release list: {e}"))?;
        Manifest::parse(&text)
    }

    /// Download the launcher named in `m`, check it, and put it in place
    /// of the running one. The caller restarts.
    pub fn update_self(&self, m: &Manifest) -> Result<(), String> {
        let tmp = std::env::temp_dir().join(format!("hw-launcher-new-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        self.fetch(&m.launcher.file, &tmp, &mut |_| {})?;
        if sha256_file(&tmp).ok().as_deref() != Some(m.launcher.sha256.as_str()) {
            let _ = std::fs::remove_file(&tmp);
            return Err("the new launcher did not download right; try again later".into());
        }
        let r = self_replace::self_replace(&tmp)
            .map_err(|e| format!("could not update the launcher: {e}"));
        let _ = std::fs::remove_file(&tmp);
        r
    }
}

impl Source for GitHub {
    fn fetch(&self, file: &str, to: &Path, progress: &mut dyn FnMut(u64)) -> Result<(), String> {
        let url = format!("{}{file}", self.base);
        let have = std::fs::metadata(to).map(|m| m.len()).unwrap_or(0);
        let mut req = self.agent.get(&url);
        if have > 0 {
            req = req.header("Range", &format!("bytes={have}-"));
        }
        let resp = req.call().map_err(|e| friendly(&url, e))?;
        // 206: the rest of a file we have part of; 200: the whole file.
        let resume = resp.status().as_u16() == 206;
        let mut out = OpenOptions::new()
            .create(true)
            .write(true)
            .append(resume)
            .truncate(!resume)
            .open(to)
            .map_err(|e| format!("could not write {}: {e}", to.display()))?;
        let mut done = if resume { have } else { 0 };
        progress(done);
        let mut body = resp.into_body().into_reader();
        let mut buf = vec![0u8; 1 << 18];
        loop {
            let n = body.read(&mut buf).map_err(|e| {
                format!(
                    "the download of {file} broke off ({e}); press the button again to carry on"
                )
            })?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| format!("could not write {}: {e}", to.display()))?;
            done += n as u64;
            progress(done);
        }
        Ok(())
    }
}

fn friendly(url: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(404) => {
            "the game files are not there right now (a new build is being put up); try again in a few minutes".into()
        }
        ureq::Error::StatusCode(416) => format!("{url}: the download is already complete"),
        ureq::Error::StatusCode(code) => format!("GitHub answered {code} for {url}"),
        other => format!("could not reach GitHub ({other}); check your internet connection"),
    }
}
