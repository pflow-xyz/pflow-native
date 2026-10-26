//! Where the store's bytes live: a tiny blob interface (get / put / delete /
//! list by key) with a file-system implementation for native builds and an
//! in-memory one for the web build and for tests.
//!
//! Keys are `/`-separated relative paths (`models/<cid>.jsonld`,
//! `index.json`). The store above never builds a path itself, so a further
//! backend (browser storage, a bucket) only has to implement these four
//! verbs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{Error, Result};

/// A blob store by key. Implementations must be safe to share between
/// threads: the store handle is process-wide.
pub trait StoreBackend: Send + Sync {
    /// The bytes under `key`, or `None` when nothing is stored there.
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    /// Stores `data` under `key`, replacing what was there. Atomic: a reader
    /// sees the old bytes or the new ones, never a torn write.
    fn put(&self, key: &str, data: &[u8]) -> Result<()>;
    /// Removes `key`; `false` when it was not there.
    fn delete(&self, key: &str) -> Result<bool>;
    /// Every key under `prefix` (a directory such as `"maps/"`), sorted.
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
    /// A human-readable location ("memory", or the root directory).
    fn location(&self) -> String;
    /// Whether the bytes outlive the process (files on disk).
    /// [`super::Store::replace_all`] refuses a persistent backend, so
    /// restoring a saved browser state never overwrites a store on disk;
    /// a backend is taken to be one unless it says otherwise.
    fn persistent(&self) -> bool {
        true
    }
}

/// Keys are relative paths with no `..`, absolute or empty component, so a
/// key can never name a file outside the store root.
pub(crate) fn check_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.starts_with('/')
        || key
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == ".." || c.contains('\\'))
    {
        return Err(Error::BadArgs(format!("invalid store key {key:?}")));
    }
    Ok(())
}

/// The process-local backend: the web build's store (no file system), and
/// what tests inject.
#[derive(Default)]
pub struct MemoryBackend {
    blobs: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn blobs(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>> {
        self.blobs.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl StoreBackend for MemoryBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        check_key(key)?;
        Ok(self.blobs().get(key).cloned())
    }

    fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        check_key(key)?;
        self.blobs().insert(key.to_string(), data.to_vec());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool> {
        check_key(key)?;
        Ok(self.blobs().remove(key).is_some())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .blobs()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }

    fn location(&self) -> String {
        "memory".into()
    }

    fn persistent(&self) -> bool {
        false
    }
}

/// Files under a root directory. Writes go to a temporary sibling first
/// and are renamed into place, so a crash or a concurrent reader never sees
/// half a file. Nothing is created until the first write.
pub struct FsBackend {
    root: PathBuf,
}

impl FsBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, key: &str) -> Result<PathBuf> {
        check_key(key)?;
        Ok(key.split('/').fold(self.root.clone(), |p, c| p.join(c)))
    }
}

fn io(path: &Path, e: std::io::Error) -> Error {
    Error::Io(format!("{}: {e}", path.display()))
}

impl StoreBackend for FsBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path(key)?;
        match std::fs::read(&path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(&path, e)),
        }
    }

    fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        let path = self.path(key)?;
        let dir = path.parent().unwrap_or(&self.root);
        std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
        // A unique temporary name per writer: two processes (the app and a
        // CLI) writing the same key each rename their own complete file.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = dir.join(format!(".{name}.tmp-{}-{seq}", std::process::id()));
        std::fs::write(&tmp, data).map_err(|e| io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            std::fs::remove_file(&tmp).ok();
            io(&path, e)
        })
    }

    fn delete(&self, key: &str) -> Result<bool> {
        let path = self.path(key)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(io(&path, e)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        // A prefix is a directory key ("maps/"), optionally followed by a
        // file-name prefix.
        let (dir_key, name_prefix) = match prefix.rfind('/') {
            Some(i) => (&prefix[..i], &prefix[i + 1..]),
            None => ("", prefix),
        };
        let dir = if dir_key.is_empty() {
            self.root.clone()
        } else {
            self.path(dir_key)?
        };
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(io(&dir, e)),
        };
        let mut out = vec![];
        for entry in entries {
            let entry = entry.map_err(|e| io(&dir, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !name.starts_with(name_prefix) {
                continue; // temporary files and other prefixes
            }
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                out.push(if dir_key.is_empty() {
                    name
                } else {
                    format!("{dir_key}/{name}")
                });
            }
        }
        out.sort();
        Ok(out)
    }

    fn location(&self) -> String {
        self.root.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(b: &dyn StoreBackend) {
        assert_eq!(b.get("index.json").unwrap(), None);
        b.put("index.json", b"{}").unwrap();
        b.put("maps/a.json", b"1").unwrap();
        b.put("maps/b.json", b"2").unwrap();
        b.put("models/x.jsonld", b"3").unwrap();
        assert_eq!(b.get("maps/a.json").unwrap().as_deref(), Some(&b"1"[..]));
        assert_eq!(b.list("maps/").unwrap(), ["maps/a.json", "maps/b.json"]);
        b.put("maps/a.json", b"9").unwrap();
        assert_eq!(b.get("maps/a.json").unwrap().as_deref(), Some(&b"9"[..]));
        assert!(b.delete("maps/a.json").unwrap());
        assert!(!b.delete("maps/a.json").unwrap());
        assert_eq!(b.list("maps/").unwrap(), ["maps/b.json"]);
        assert!(b.get("../escape").is_err());
        assert!(b.put("/abs", b"").is_err());
    }

    #[test]
    fn memory_backend() {
        exercise(&MemoryBackend::new());
        assert!(!MemoryBackend::new().persistent());
        assert!(FsBackend::new("unused").persistent());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn fs_backend_is_atomic_and_lists_without_temporaries() {
        let dir = std::env::temp_dir().join(format!(
            "pflow-native-store-backend-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        let b = FsBackend::new(&dir);
        assert_eq!(b.list("maps/").unwrap(), Vec::<String>::new());
        exercise(&b);
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("maps"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "no temporary files remain");
        std::fs::remove_dir_all(&dir).ok();
    }
}
