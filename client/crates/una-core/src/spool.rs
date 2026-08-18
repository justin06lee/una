//! Failed-upload spool: WAV files kept in the cache dir so the last dictation
//! can be retried from the tray. Pruned to the newest [`KEEP`] files.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const KEEP: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum SpoolError {
    #[error("could not determine a cache directory for this platform")]
    NoProjectDirs,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Default spool directory (`{cache_dir}/spool`).
pub fn spool_dir() -> Result<PathBuf, SpoolError> {
    let dirs =
        directories::ProjectDirs::from("sh", "tenet", "una").ok_or(SpoolError::NoProjectDirs)?;
    Ok(dirs.cache_dir().join("spool"))
}

/// Save a WAV into the default spool dir and prune old entries.
pub fn save(wav: &[u8]) -> Result<PathBuf, SpoolError> {
    let dir = spool_dir()?;
    save_in(&dir, wav)
}

pub fn save_in(dir: &Path, wav: &[u8]) -> Result<PathBuf, SpoolError> {
    std::fs::create_dir_all(dir)?;
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    // A zero-padded counter suffix keeps names unique (and lexically ordered)
    // within one millisecond.
    let mut n = 0u32;
    let mut path = dir.join(format!("utt-{ms:015}-{n:03}.wav"));
    while path.exists() {
        n += 1;
        path = dir.join(format!("utt-{ms:015}-{n:03}.wav"));
    }
    std::fs::write(&path, wav)?;
    prune_in(dir, KEEP)?;
    Ok(path)
}

/// All spooled files, newest first.
pub fn list() -> Result<Vec<PathBuf>, SpoolError> {
    let dir = spool_dir()?;
    list_in(&dir)
}

pub fn list_in(dir: &Path) -> Result<Vec<PathBuf>, SpoolError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|e| e == "wav").unwrap_or(false)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("utt-"))
                    .unwrap_or(false)
        })
        .collect();
    // Names embed a zero-padded timestamp, so lexical order == time order.
    entries.sort();
    entries.reverse();
    Ok(entries)
}

/// The newest spooled WAV, if any.
pub fn latest() -> Result<Option<(PathBuf, Vec<u8>)>, SpoolError> {
    let dir = spool_dir()?;
    latest_in(&dir)
}

pub fn latest_in(dir: &Path) -> Result<Option<(PathBuf, Vec<u8>)>, SpoolError> {
    match list_in(dir)?.into_iter().next() {
        Some(path) => {
            let bytes = std::fs::read(&path)?;
            Ok(Some((path, bytes)))
        }
        None => Ok(None),
    }
}

pub fn prune_in(dir: &Path, keep: usize) -> Result<(), SpoolError> {
    let entries = list_in(dir)?;
    for old in entries.into_iter().skip(keep) {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_latest_prune() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        for i in 0..7u8 {
            save_in(dir, &[i; 4]).unwrap();
        }
        let listed = list_in(dir).unwrap();
        assert_eq!(listed.len(), KEEP, "pruned to {KEEP}");
        let (path, bytes) = latest_in(dir).unwrap().unwrap();
        assert_eq!(bytes, vec![6u8; 4], "latest is the most recent write");
        assert_eq!(listed[0], path);
    }

    #[test]
    fn empty_dir_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_in(dir.path()).unwrap().is_empty());
        assert!(latest_in(dir.path()).unwrap().is_none());
    }
}
