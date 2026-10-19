//! Offline collection preferences, separate from cartridges and their save memory.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: PathBuf,
    pub favourite: bool,
    pub labels: Vec<String>,
    pub last_used: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    revision: u8,
    sequence: u64,
    pub entries: Vec<Entry>,
}

impl Default for Collection {
    fn default() -> Self {
        Self {
            revision: 1,
            sequence: 0,
            entries: Vec::new(),
        }
    }
}

pub struct LibraryStore {
    path: PathBuf,
    collection: Collection,
    expected: Option<Vec<u8>>,
    error: Option<String>,
}

impl LibraryStore {
    /// Loading never creates or replaces a file. Corruption leaves a visible read-only store.
    pub fn open(path: PathBuf) -> Self {
        let mut store = Self {
            path,
            collection: Collection::default(),
            expected: None,
            error: None,
        };
        let _ = store.reload();
        store
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let loaded = read_metadata(&self.path).and_then(|bytes| {
            let collection = match &bytes {
                Some(bytes) => decode(bytes)?,
                None => Collection::default(),
            };
            Ok((collection, bytes))
        });
        match loaded {
            Ok((collection, bytes)) => {
                self.collection = collection;
                self.expected = bytes;
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn collection(&self) -> &Collection {
        &self.collection
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn register(&mut self, paths: &[PathBuf]) -> Result<usize, String> {
        let paths = paths
            .iter()
            .map(|path| cartridge_path(path))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut next = self.collection.clone();
        let mut added = 0;
        for path in paths {
            if !next.entries.iter().any(|entry| entry.path == path) {
                next.entries.push(Entry {
                    path,
                    favourite: false,
                    labels: Vec::new(),
                    last_used: 0,
                });
                added += 1;
            }
        }
        self.commit(next)?;
        Ok(added)
    }

    pub fn preferences(
        &mut self,
        path: &Path,
        favourite: bool,
        labels: &[String],
    ) -> Result<(), String> {
        let path = if self
            .collection
            .entries
            .iter()
            .any(|entry| entry.path == path)
        {
            path.to_path_buf()
        } else {
            cartridge_path(path)?
        };
        let labels = normalized_labels(labels)?;
        let mut next = self.collection.clone();
        let entry = entry_or_insert(&mut next, path);
        entry.favourite = favourite;
        entry.labels = labels;
        self.commit(next)
    }

    pub fn launched(&mut self, path: &Path) -> Result<(), String> {
        let path = cartridge_path(path)?;
        let mut next = self.collection.clone();
        next.sequence = next
            .sequence
            .checked_add(1)
            .ok_or("library recent counter is exhausted")?;
        let sequence = next.sequence;
        entry_or_insert(&mut next, path).last_used = sequence;
        self.commit(next)
    }

    /// Removes a bookmark only. Does not inspect, delete or rename the referenced cartridge.
    pub fn forget(&mut self, path: &Path) -> Result<(), String> {
        let mut next = self.collection.clone();
        next.entries.retain(|entry| entry.path != path);
        self.commit(next)
    }

    fn commit(&mut self, mut next: Collection) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(format!(
                "library is read-only until a successful reload: {error}"
            ));
        }
        next.entries
            .sort_by(|left, right| left.path.cmp(&right.path));
        validate(&next)?;
        let mut bytes = serde_json::to_vec_pretty(&next).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        if bytes.len() > MAX_BYTES {
            return Err("library metadata exceeds 1 MiB".to_owned());
        }
        let parent = self
            .path
            .parent()
            .ok_or("library metadata has no parent directory")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let lock_path = self.path.with_extension("lock");
        regular_or_absent(&lock_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|error| error.to_string())?;
        lock.try_lock()
            .map_err(|error| format!("library is busy in another Studio: {error}"))?;
        if read_metadata(&self.path)? != self.expected {
            return Err(
                "library changed outside this Studio; reload before editing preferences".to_owned(),
            );
        }
        if self.collection == next {
            return Ok(());
        }
        let temporary = self.path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        let result = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| fs::rename(&temporary, &self.path));
        if let Err(error) = result {
            // The create_new file is owned exclusively by this transaction.
            let _ = fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        self.collection = next;
        self.expected = Some(bytes);
        Ok(())
    }
}

fn entry_or_insert(collection: &mut Collection, path: PathBuf) -> &mut Entry {
    let index = collection
        .entries
        .iter()
        .position(|entry| entry.path == path)
        .unwrap_or_else(|| {
            collection.entries.push(Entry {
                path,
                favourite: false,
                labels: Vec::new(),
                last_used: 0,
            });
            collection.entries.len() - 1
        });
    &mut collection.entries[index]
}

pub fn cartridge_path(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_dir() {
        path.join("cart.m01w")
    } else {
        path.to_path_buf()
    };
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    if !path.is_file() || !supported_path(&path) || !valid_path(&path) {
        return Err("library requires an existing UTF-8 .m01w, .m01c or .m01c.png file".to_owned());
    }
    Ok(path)
}

pub fn supported_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".m01w") || name.ends_with(".m01c") || name.ends_with(".m01c.png")
        })
}

fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .to_str()
            .is_some_and(|path| path.len() <= 4096 && !path.chars().any(char::is_control))
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
}

pub fn normalized_labels(labels: &[String]) -> Result<Vec<String>, String> {
    if labels.len() > 8 {
        return Err("use at most eight library labels".to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for label in labels {
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 32 || label.chars().any(char::is_control) {
            return Err("labels need 1–32 printable characters".to_owned());
        }
        if seen.insert(label.to_lowercase()) {
            output.push(label.to_owned());
        }
    }
    Ok(output)
}

fn decode(bytes: &[u8]) -> Result<Collection, String> {
    let collection: Collection = serde_json::from_slice(bytes)
        .map_err(|error| format!("cannot read library metadata: {error}"))?;
    validate(&collection)?;
    Ok(collection)
}

fn validate(collection: &Collection) -> Result<(), String> {
    if collection.revision != 1 || collection.entries.len() > MAX_ENTRIES {
        return Err("invalid library revision or entry count (maximum 1,024)".to_owned());
    }
    let mut paths = BTreeSet::new();
    for entry in &collection.entries {
        if !valid_path(&entry.path)
            || !supported_path(&entry.path)
            || !paths.insert(&entry.path)
            || entry.last_used > collection.sequence
            || normalized_labels(&entry.labels)? != entry.labels
        {
            return Err(
                "invalid library path, duplicate entry, recent counter or labels".to_owned(),
            );
        }
    }
    Ok(())
}

fn regular_or_absent(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(
            "library metadata and lock must be ordinary files, not symlinks or directories"
                .to_owned(),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn read_metadata(path: &Path) -> Result<Option<Vec<u8>>, String> {
    regular_or_absent(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("library metadata exceeds 1 MiB".to_owned());
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Temporary(PathBuf);
    impl Temporary {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "mod01-library-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn cart(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            crate::project::NativeProject::create(&path, "library-fixture").unwrap();
            cartridge_path(&path).unwrap()
        }
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn favourites_labels_recents_and_forgetting_persist_without_touching_cartridges() {
        let temporary = Temporary::new();
        let cart = temporary.cart("game");
        let original = fs::read(&cart).unwrap();
        let path = temporary.0.join("settings/library.json");
        let mut store = LibraryStore::open(path.clone());
        assert_eq!(store.path(), path);
        assert!(store.error().is_none());
        assert!(
            !path.parent().unwrap().exists(),
            "reading cannot create metadata"
        );
        assert_eq!(
            store
                .register(&[cart.parent().unwrap().to_path_buf(), cart.clone()])
                .unwrap(),
            1
        );
        store
            .preferences(
                &cart,
                true,
                &[" Jam ".to_owned(), "jam".to_owned(), "Puzzle".to_owned()],
            )
            .unwrap();
        store.launched(&cart).unwrap();
        let mut reopened = LibraryStore::open(path);
        assert_eq!(store.collection(), reopened.collection());
        assert_eq!(reopened.collection.entries[0].labels, ["Jam", "Puzzle"]);
        assert!(reopened.collection.entries[0].favourite);
        assert_eq!(reopened.collection.entries[0].last_used, 1);
        let bytes = fs::read(reopened.path()).unwrap();
        reopened.register(std::slice::from_ref(&cart)).unwrap();
        assert_eq!(fs::read(reopened.path()).unwrap(), bytes);
        reopened.forget(&cart).unwrap();
        assert!(
            LibraryStore::open(reopened.path().to_owned())
                .collection
                .entries
                .is_empty()
        );
        assert_eq!(fs::read(cart).unwrap(), original);
        assert!(
            !fs::read_dir(reopened.path().parent().unwrap())
                .unwrap()
                .any(|file| file
                    .unwrap()
                    .path()
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("tmp-"))
        );
    }

    #[test]
    fn malformed_oversized_unknown_and_duplicate_metadata_remain_untouched_and_read_only() {
        let temporary = Temporary::new();
        let cart = temporary.cart("game");
        let path = temporary.0.join("library.json");
        let entry = Entry {
            path: cart.clone(),
            favourite: false,
            labels: Vec::new(),
            last_used: 0,
        };
        let cases = [
            b"{not JSON".to_vec(),
            vec![b' '; MAX_BYTES + 1],
            br#"{"revision":0,"sequence":0,"entries":[]}"#.to_vec(),
            br#"{"revision":1,"sequence":0,"entries":[],"unknown":true}"#.to_vec(),
            serde_json::to_vec(&Collection {
                revision: 1,
                sequence: 0,
                entries: vec![entry.clone(), entry.clone()],
            })
            .unwrap(),
            serde_json::to_vec(&Collection {
                revision: 1,
                sequence: 0,
                entries: vec![Entry {
                    last_used: 1,
                    ..entry
                }],
            })
            .unwrap(),
        ];
        for invalid in cases {
            fs::write(&path, &invalid).unwrap();
            let mut store = LibraryStore::open(path.clone());
            assert!(store.error().is_some());
            assert!(store.preferences(&cart, true, &[]).is_err());
            assert!(store.reload().is_err());
            assert_eq!(fs::read(&path).unwrap(), invalid);
        }
        fs::remove_file(&path).unwrap();
        let mut store = LibraryStore::open(path);
        store.register(&[cart]).unwrap();
        assert!(store.error().is_none());
    }

    #[test]
    fn stale_studios_and_locked_or_externally_modified_metadata_cannot_lose_preferences() {
        let temporary = Temporary::new();
        let cart = temporary.cart("game");
        let path = temporary.0.join("library.json");
        let mut first = LibraryStore::open(path.clone());
        let mut stale = LibraryStore::open(path.clone());
        first
            .preferences(&cart, true, &["kept".to_owned()])
            .unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(
            stale
                .register(std::slice::from_ref(&cart))
                .unwrap_err()
                .contains("reload")
        );
        assert!(stale.collection.entries.is_empty());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        stale.reload().unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        lock.try_lock().unwrap();
        assert!(
            stale
                .preferences(&cart, false, &[])
                .unwrap_err()
                .contains("busy")
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        drop(lock); // The OS lock disappears with its handle; no stale PID-file recovery is needed.
        stale.preferences(&cart, false, &[]).unwrap();
        assert!(!stale.collection.entries[0].favourite);
        let previous = stale.collection.clone();
        fs::write(&path, b"broken outside Studio").unwrap();
        assert!(stale.reload().is_err());
        assert_eq!(stale.collection, previous);
        assert!(stale.preferences(&cart, true, &[]).is_err());
        assert_eq!(fs::read(path).unwrap(), b"broken outside Studio");
    }

    #[test]
    fn invalid_batches_and_limits_are_transactional_and_unavailable_bookmarks_remain_editable() {
        let temporary = Temporary::new();
        let cart = temporary.cart("game");
        let path = temporary.0.join("library.json");
        let mut store = LibraryStore::open(path.clone());
        assert!(
            store
                .register(&[cart.clone(), temporary.0.join("missing.m01w")])
                .is_err()
        );
        assert!(!path.exists() && store.collection.entries.is_empty());
        store.register(std::slice::from_ref(&cart)).unwrap();
        let bytes = fs::read(&path).unwrap();
        for labels in [
            vec!["X".repeat(33)],
            vec!["\n".to_owned()],
            vec!["bad\nlabel".to_owned()],
            vec!["x".to_owned(); 9],
        ] {
            assert!(store.preferences(&cart, true, &labels).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let saved = fs::read(&cart).unwrap();
        fs::remove_file(&cart).unwrap(); // Only this test-owned cartridge is made unavailable.
        store
            .preferences(&cart, true, &["offline drive".to_owned()])
            .unwrap();
        assert!(LibraryStore::open(path).collection.entries[0].favourite);
        store.forget(&cart).unwrap();
        fs::write(cart, saved).unwrap();
        let mut collection = Collection {
            entries: (0..=MAX_ENTRIES)
                .map(|index| Entry {
                    path: temporary.0.join(format!("{index}.m01w")),
                    favourite: false,
                    labels: Vec::new(),
                    last_used: 0,
                })
                .collect(),
            ..Default::default()
        };
        assert!(validate(&collection).is_err());
        collection.entries = vec![Entry {
            path: PathBuf::from("../outside.m01w"),
            favourite: false,
            labels: Vec::new(),
            last_used: 0,
        }];
        assert!(validate(&collection).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn metadata_and_lock_symlinks_are_never_followed_or_replaced() {
        let temporary = Temporary::new();
        let cart = temporary.cart("game");
        let outside = temporary.0.join("keep.txt");
        fs::write(&outside, b"keep this").unwrap();
        let path = temporary.0.join("library.json");
        std::os::unix::fs::symlink(&outside, &path).unwrap();
        let mut store = LibraryStore::open(path.clone());
        assert!(store.error().is_some());
        assert!(store.register(std::slice::from_ref(&cart)).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"keep this");
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&outside, path.with_extension("lock")).unwrap();
        store.reload().unwrap();
        assert!(store.register(&[cart]).is_err());
        assert!(!path.exists());
        assert_eq!(fs::read(outside).unwrap(), b"keep this");
    }
}
