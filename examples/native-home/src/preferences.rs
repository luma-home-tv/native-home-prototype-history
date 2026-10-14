//! Dashboard-only preferences in an app-owned durable directory. Directory descriptors and
//! openat/renameat keep atomic writes inside that directory even if its parent is renamed.
use crate::{model::valid_app_id, shell::Preferences};
use serde::{Deserialize, Serialize};
use std::{
    ffi::CString,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
const NAME: &std::ffi::CStr = c"preferences.json";
const MAX_BYTES: u64 = 64 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(0);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    preferences: Preferences,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn validate(p: &Preferences) -> io::Result<()> {
    if p.favourites.len() > 256 || p.favourites.iter().any(|id| !valid_app_id(id)) {
        return Err(invalid("Invalid favourite identity"));
    }
    Ok(())
}
fn private_file(file: &File) -> io::Result<()> {
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
        || meta.nlink() != 1
    {
        return Err(invalid("Preference file is not privately owned"));
    }
    if meta.len() > MAX_BYTES {
        return Err(invalid("Preference file too large"));
    }
    Ok(())
}
pub struct Store {
    dir: File,
}
impl Store {
    pub fn open(path: &Path) -> io::Result<Self> {
        if unsafe { libc::geteuid() } == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Refusing root preference writes",
            ));
        }
        let path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| invalid("Invalid state path"))?;
        let made = unsafe { libc::mkdir(path.as_ptr(), 0o700) };
        if made < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let dir = unsafe { File::from_raw_fd(fd) };
        let meta = dir.metadata()?;
        if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(invalid("State directory is not privately owned"));
        }
        Ok(Self { dir })
    }
    pub fn load(&self) -> io::Result<Option<Preferences>> {
        let fd = unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                NAME.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            let err = io::Error::last_os_error();
            return if err.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(err)
            };
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        private_file(&file)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES as usize {
            return Err(invalid("Preference file grew past its limit"));
        }
        let record: Record =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Malformed preference record"))?;
        if record.schema != 1 {
            return Err(invalid("Unsupported preference schema"));
        }
        validate(&record.preferences)?;
        Ok(Some(record.preferences))
    }
    pub fn save(&self, prefs: &Preferences) -> io::Result<()> {
        validate(prefs)?;
        // Never replace an unreadable, foreign or newer-schema state file with defaults.
        self.load()?;
        let temp = CString::new(format!(
            ".preferences-{}-{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ))
        .unwrap();
        let fd = unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                temp.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let write = (|| {
            serde_json::to_writer(
                &mut file,
                &Record {
                    schema: 1,
                    preferences: prefs.clone(),
                },
            )?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            if unsafe {
                libc::renameat(
                    self.dir.as_raw_fd(),
                    temp.as_ptr(),
                    self.dir.as_raw_fd(),
                    NAME.as_ptr(),
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
            self.dir.sync_all()?;
            Ok(())
        })();
        if write.is_err() {
            unsafe { libc::unlinkat(self.dir.as_raw_fd(), temp.as_ptr(), 0) };
        }
        write
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        os::unix::fs::{symlink, PermissionsExt},
    };
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "native-home-pref-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn prefs() -> Preferences {
        Preferences {
            initialized: true,
            favourites: BTreeSet::from(["com.example.video".into()]),
        }
    }
    #[test]
    fn saved_favourites_survive_closing_and_reopening_and_keep_explicit_empty_state() {
        let tmp = Scratch::new();
        let path = tmp.0.join("state");
        let store = Store::open(&path).unwrap();
        assert!(store.load().unwrap().is_none());
        store.save(&prefs()).unwrap();
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened.load().unwrap().unwrap().favourites,
            prefs().favourites
        );
        let empty = Preferences {
            initialized: true,
            favourites: BTreeSet::new(),
        };
        reopened.save(&empty).unwrap();
        drop(reopened);
        let loaded = Store::open(&path).unwrap().load().unwrap().unwrap();
        assert!(loaded.initialized && loaded.favourites.is_empty());
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
    }
    #[test]
    fn symlinked_directory_or_file_cannot_redirect_a_save() {
        let tmp = Scratch::new();
        let outside = tmp.0.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o700)).unwrap();
        symlink(&outside, tmp.0.join("state-link")).unwrap();
        assert!(Store::open(&tmp.0.join("state-link")).is_err());
        let store = Store::open(&tmp.0.join("state")).unwrap();
        let target = outside.join("sentinel");
        std::fs::write(&target, b"unchanged").unwrap();
        symlink(&target, tmp.0.join("state/preferences.json")).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&prefs()).is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"unchanged");
    }
    #[test]
    fn future_schema_or_corrupt_state_is_preserved_and_invalid_id_refused() {
        let tmp = Scratch::new();
        let path = tmp.0.join("state");
        let store = Store::open(&path).unwrap();
        store.save(&prefs()).unwrap();
        let target = path.join("preferences.json");
        let original = b"{\"schema\":2,\"preferences\":{\"initialized\":true,\"favourites\":[]}}";
        std::fs::write(&target, original).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&prefs()).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), original);
        std::fs::write(&target, b"bad-json").unwrap();
        assert!(store.save(&prefs()).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"bad-json");
        std::fs::remove_file(target).unwrap();
        let bad = Preferences {
            initialized: true,
            favourites: BTreeSet::from(["../../other-app".into()]),
        };
        assert!(store.save(&bad).is_err());
    }
    #[test]
    fn open_directory_descriptor_keeps_save_in_its_original_directory() {
        let tmp = Scratch::new();
        let original = tmp.0.join("state");
        let store = Store::open(&original).unwrap();
        let moved = tmp.0.join("moved");
        std::fs::rename(&original, &moved).unwrap();
        std::fs::create_dir(&original).unwrap();
        store.save(&prefs()).unwrap();
        assert!(moved.join("preferences.json").exists());
        assert!(!original.join("preferences.json").exists());
    }
}

/// Disk work runs off the render thread. Acknowledgements report readback, not merely queueing.
pub struct Persistence {
    sender: std::sync::mpsc::SyncSender<Preferences>,
    pending: Option<Preferences>,
    replies: std::sync::mpsc::Receiver<Result<Preferences, String>>,
    worker: std::thread::JoinHandle<()>,
}
impl Persistence {
    pub fn start(path: &Path) -> io::Result<(Self, Option<Preferences>)> {
        let store = Store::open(path)?;
        let loaded = store.load()?;
        let (sender, requests) = std::sync::mpsc::sync_channel::<Preferences>(1);
        let (ack, replies) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("home-preferences".into())
            .spawn(move || {
                while let Ok(mut prefs) = requests.recv() {
                    // A fast pin/unpin burst saves its final state instead of blocking animation.
                    while let Ok(newest) = requests.try_recv() {
                        prefs = newest;
                    }
                    let result = (|| {
                        store.save(&prefs)?;
                        if store.load()? != Some(prefs.clone()) {
                            return Err(invalid("Preference readback mismatch"));
                        }
                        Ok(prefs)
                    })()
                    .map_err(|e: io::Error| e.to_string());
                    if ack.send(result).is_err() {
                        break;
                    }
                }
            })?;
        Ok((
            Self {
                sender,
                pending: None,
                replies,
                worker,
            },
            loaded,
        ))
    }
    fn flush(&mut self) -> Result<(), String> {
        let Some(prefs) = self.pending.take() else {
            return Ok(());
        };
        match self.sender.try_send(prefs) {
            Ok(()) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Full(prefs)) => {
                self.pending = Some(prefs);
                Ok(())
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                Err("Preference writer stopped".into())
            }
        }
    }
    pub fn queue(&mut self, prefs: &Preferences) -> Result<(), String> {
        validate(prefs).map_err(|e| e.to_string())?;
        self.pending = Some(prefs.clone());
        self.flush()
    }
    pub fn poll(&mut self) -> Vec<Result<Preferences, String>> {
        let mut results = self.replies.try_iter().collect::<Vec<_>>();
        if let Err(error) = self.flush() {
            results.push(Err(error));
        }
        results
    }
    pub fn finish(self) -> Vec<Result<Preferences, String>> {
        let Self {
            sender,
            pending,
            replies,
            worker,
        } = self;
        let final_failed = pending.is_some_and(|prefs| sender.send(prefs).is_err());
        drop(sender);
        if worker.join().is_err() {
            return vec![Err("Preference writer panicked".into())];
        }
        let mut results = replies.try_iter().collect::<Vec<_>>();
        if final_failed {
            results.push(Err("Final preference write failed".into()));
        }
        results
    }
}
#[cfg(test)]
mod writer_tests {
    use super::*;
    #[test]
    fn asynchronous_writer_finishes_with_latest_readback_and_reopens_it() {
        let root = std::env::temp_dir().join(format!(
            "native-home-writer-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let (mut worker, loaded) = Persistence::start(&root).unwrap();
        assert!(loaded.is_none());
        let first = Preferences {
            initialized: true,
            favourites: std::collections::BTreeSet::from(["com.example.first".into()]),
        };
        let last = Preferences {
            initialized: true,
            favourites: std::collections::BTreeSet::new(),
        };
        worker.queue(&first).unwrap();
        worker.queue(&last).unwrap();
        let replies = worker.finish();
        assert_eq!(replies.last().unwrap().as_ref().unwrap(), &last);
        assert_eq!(Store::open(&root).unwrap().load().unwrap(), Some(last));
        std::fs::remove_dir_all(root).unwrap();
    }
}
