//! Local `.m01save` export and import.

use serde::{Deserialize, Serialize};
use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SAVE_BYTES: usize = 8192;
const MAX_CONTAINER_BYTES: usize = MAX_SAVE_BYTES * 2 + 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SaveExport {
    revision: u8,
    cartridge_id: String,
    schema_version: u16,
    byte_length: usize,
    checksum: String,
    bytes: String,
}

pub fn export(id: &str, schema: u16, bytes: &[u8]) -> Result<Vec<u8>, String> {
    modl_core::cartridge::cartridge_file_stem(id).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_SAVE_BYTES {
        return Err("save exceeds 8 KiB".to_owned());
    }
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut hex, "{byte:02x}").map_err(|error| error.to_string())?;
    }
    serde_json::to_vec(&SaveExport {
        revision: 1,
        cartridge_id: id.to_owned(),
        schema_version: schema,
        byte_length: bytes.len(),
        checksum: checksum(bytes),
        bytes: hex,
    })
    .map_err(|error| error.to_string())
}

pub fn import(id: &str, source: &[u8]) -> Result<(u16, Vec<u8>), String> {
    modl_core::cartridge::cartridge_file_stem(id).map_err(|error| error.to_string())?;
    if source.is_empty() || source.len() > MAX_CONTAINER_BYTES {
        return Err("save container size is invalid".to_owned());
    }
    let container: SaveExport =
        serde_json::from_slice(source).map_err(|error| error.to_string())?;
    if container.revision != 1
        || container.cartridge_id != id
        || container.byte_length > MAX_SAVE_BYTES
    {
        return Err("save container identity or schema is invalid".to_owned());
    }
    if container.bytes.len() != container.byte_length * 2
        || !container
            .bytes
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("save bytes are not canonical lowercase hex".to_owned());
    }
    let bytes = (0..container.bytes.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&container.bytes[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if checksum(&bytes) != container.checksum {
        return Err("save container checksum failed".to_owned());
    }
    Ok((container.schema_version, bytes))
}

pub fn storage_path(data: &Path, id: &str) -> Result<PathBuf, String> {
    let stem = modl_core::cartridge::cartridge_file_stem(id).map_err(|error| error.to_string())?;
    Ok(data.join("saves").join(format!("{stem}.bin")))
}

/// Keeps ten valid backups for this ID only. The caller must complete this before
/// replacing/resetting/deleting a save. Existing old-layout files are never migrated.
pub fn backup(save_file: &Path, id: &str, schema: u16, bytes: &[u8]) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    backup_at(save_file, id, schema, bytes, stamp)
}

fn backup_at(
    save_file: &Path,
    id: &str,
    schema: u16,
    bytes: &[u8],
    stamp: u128,
) -> Result<PathBuf, String> {
    let stem = modl_core::cartridge::cartridge_file_stem(id).map_err(|error| error.to_string())?;
    let exported = export(id, schema, bytes)?;
    let directory = save_file
        .parent()
        .ok_or("invalid save path")?
        .join("recovery")
        .join(format!("{stem}.saves"));
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let metadata = fs::symlink_metadata(&directory).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("save recovery must use an ordinary per-cartridge directory".to_owned());
    }
    let created = create_recovery(&directory, stamp, &exported)?;
    let mut revisions = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if let Some(revision) = owned_revision(&entry, id) {
            revisions.push(revision);
        }
    }
    // Clock corrections must never prune the recovery copy just created for the
    // destructive operation. Other IDs, malformed files and symlinks are not ours.
    revisions.sort_by_key(|(stamp, path)| (path != &created, std::cmp::Reverse(*stamp)));
    for (_, old) in revisions.iter().skip(10) {
        fs::remove_file(old).map_err(|error| error.to_string())?;
    }
    Ok(created)
}

fn create_recovery(directory: &Path, stamp: u128, source: &[u8]) -> Result<PathBuf, String> {
    for offset in 0..32_u128 {
        let stamp = stamp
            .checked_add(offset)
            .ok_or("save recovery counter overflow")?;
        let path = directory.join(format!("{stamp:020}.m01save"));
        // create_new atomically refuses existing files and even dangling symlinks.
        let file = OpenOptions::new().write(true).create_new(true).open(&path);
        match file {
            Ok(mut file) => {
                file.write_all(source)
                    .and_then(|()| file.sync_all())
                    .map_err(|error| error.to_string())?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("save recovery names are busy; no existing backup was overwritten".to_owned())
}

fn owned_revision(entry: &fs::DirEntry, id: &str) -> Option<(u128, PathBuf)> {
    if !entry.file_type().ok()?.is_file() {
        return None;
    }
    let name = entry.file_name();
    let stamp = name.to_str()?.strip_suffix(".m01save")?;
    if !(20..=39).contains(&stamp.len()) || !stamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let stamp = stamp.parse().ok()?;
    let file = File::open(entry.path()).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CONTAINER_BYTES as u64 {
        return None;
    }
    let mut source = Vec::new();
    file.take(MAX_CONTAINER_BYTES as u64 + 1)
        .read_to_end(&mut source)
        .ok()?;
    import(id, &source).ok()?;
    Some((stamp, entry.path()))
}

fn checksum(bytes: &[u8]) -> String {
    let mut value = !0_u32;
    for &byte in bytes {
        value ^= u32::from(byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (if value & 1 == 1 { 0xedb8_8320 } else { 0 });
        }
    }
    format!("{:08x}", !value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mod01-save-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    fn recovery_files(save_file: &Path, id: &str) -> Vec<PathBuf> {
        let stem = modl_core::cartridge::cartridge_file_stem(id).unwrap();
        let directory = save_file
            .parent()
            .unwrap()
            .join("recovery")
            .join(format!("{stem}.saves"));
        fs::read_dir(directory)
            .unwrap()
            .map(Result::unwrap)
            .filter_map(|entry| owned_revision(&entry, id).map(|(_, path)| path))
            .collect()
    }

    #[test]
    fn save_paths_are_portable_and_old_development_files_are_never_moved_or_loaded_implicitly() {
        let root = temporary("paths");
        for id in [
            "con",
            "con.foo",
            "aux",
            "nul",
            "com1",
            "trailing.",
            "abc",
            "abc-def",
        ] {
            let path = storage_path(&root, id).unwrap();
            assert_eq!(
                path,
                root.join("saves")
                    .join(format!("cart-{}.bin", id.replace('.', "_")))
            );
            let exported = export(id, 7, &[1, 2, 3]).unwrap();
            assert_eq!(import(id, &exported).unwrap(), (7, vec![1, 2, 3]));
            assert!(import("other-cart", &exported).is_err());
        }
        fs::create_dir(root.join("saves")).unwrap();
        let old = root.join("saves/example.game.bin");
        fs::write(&old, b"existing development bytes").unwrap();
        let new = storage_path(&root, "example.game").unwrap();
        assert!(!new.exists());
        backup_at(&new, "example.game", 1, &[], 10).unwrap();
        assert_eq!(fs::read(&old).unwrap(), b"existing development bytes");
        assert!(!new.exists());
        for id in ["../outside", "a/b", "ab", "AUX"] {
            assert!(storage_path(&root.join("invalid"), id).is_err());
            assert!(export(id, 1, &[]).is_err());
            assert!(import(id, b"{}").is_err());
        }
        assert!(!root.join("invalid").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_retention_is_per_identity_not_a_filename_prefix_and_preserves_foreign_or_invalid_files()
     {
        let root = temporary("retention");
        let first = storage_path(&root, "abc").unwrap();
        let sibling = storage_path(&root, "abc-def").unwrap();
        let other = backup_at(&sibling, "abc-def", 2, &[99], 100).unwrap();
        let other_bytes = fs::read(&other).unwrap();
        fs::write(&first, [12]).unwrap();
        fs::write(&sibling, [13]).unwrap();
        let first_backup = backup_at(&first, "abc", 1, &[0], 200).unwrap();
        let directory = first_backup.parent().unwrap().to_path_buf();
        let foreign = directory.join("00000000000000009999.m01save");
        let invalid = directory.join("00000000000000009998.m01save");
        let oversized = directory.join("00000000000000009997.m01save");
        let foreign_bytes = export("foreign-cart", 1, &[78]).unwrap();
        fs::write(&foreign, &foreign_bytes).unwrap();
        fs::write(&invalid, b"not a save").unwrap();
        fs::write(&oversized, vec![7; MAX_CONTAINER_BYTES + 1]).unwrap();
        let mut backups = vec![first_backup];
        for index in 1..12_u8 {
            backups.push(backup_at(&first, "abc", 1, &[index], 200 + u128::from(index)).unwrap());
        }
        assert!(!backups[0].exists() && !backups[1].exists());
        assert!(backups[2..].iter().all(|path| path.is_file()));
        assert_eq!(recovery_files(&first, "abc").len(), 10);
        assert_eq!(recovery_files(&sibling, "abc-def").len(), 1);
        assert_eq!(fs::read(&other).unwrap(), other_bytes);
        assert_eq!(fs::read(&foreign).unwrap(), foreign_bytes);
        assert_eq!(fs::read(&invalid).unwrap(), b"not a save");
        assert_eq!(
            fs::metadata(&oversized).unwrap().len(),
            (MAX_CONTAINER_BYTES + 1) as u64
        );
        assert_eq!(fs::read(first).unwrap(), [12]);
        assert_eq!(fs::read(sibling).unwrap(), [13]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_creation_handles_identical_timestamps_clock_corrections_and_busy_names_without_overwriting()
     {
        let root = temporary("clock");
        let save = storage_path(&root, "clock-cart").unwrap();
        let first = backup_at(&save, "clock-cart", 1, &[1], 100).unwrap();
        let second = backup_at(&save, "clock-cart", 1, &[2], 100).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            import("clock-cart", &fs::read(&first).unwrap()).unwrap().1,
            [1]
        );
        assert_eq!(
            import("clock-cart", &fs::read(&second).unwrap()).unwrap().1,
            [2]
        );
        for index in 0..12_u8 {
            backup_at(&save, "clock-cart", 1, &[index], 200 + u128::from(index)).unwrap();
        }
        let corrected = backup_at(&save, "clock-cart", 1, &[77], 1).unwrap();
        assert!(
            corrected.is_file(),
            "the immediate pre-reset copy survives a clock rollback"
        );
        assert_eq!(recovery_files(&save, "clock-cart").len(), 10);
        let directory = corrected.parent().unwrap();
        for stamp in 500..532 {
            fs::write(directory.join(format!("{stamp:020}.m01save")), b"occupied").unwrap();
        }
        assert!(
            backup_at(&save, "clock-cart", 1, &[88], 500)
                .unwrap_err()
                .contains("busy")
        );
        for stamp in 500..532 {
            assert_eq!(
                fs::read(directory.join(format!("{stamp:020}.m01save"))).unwrap(),
                b"occupied"
            );
        }
        assert_eq!(
            import("clock-cart", &fs::read(corrected).unwrap())
                .unwrap()
                .1,
            [77]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn recovery_rejects_a_linked_identity_directory_and_never_replaces_or_prunes_linked_files() {
        use std::os::unix::fs::symlink;
        let root = temporary("links");
        let save = storage_path(&root, "con.foo").unwrap();
        let recovery = root.join("saves/recovery");
        fs::create_dir_all(&recovery).unwrap();
        let foreign = root.join("foreign");
        fs::create_dir(&foreign).unwrap();
        let target = foreign.join("keep.txt");
        fs::write(&target, b"never touch this").unwrap();
        let directory = recovery.join("cart-con_foo.saves");
        symlink(&foreign, &directory).unwrap();
        assert!(backup_at(&save, "con.foo", 1, &[], 100).is_err());
        assert_eq!(fs::read_dir(&foreign).unwrap().count(), 1);
        fs::remove_file(&directory).unwrap();
        fs::create_dir(&directory).unwrap();
        let dangling = directory.join("00000000000000000100.m01save");
        let absent = foreign.join("absent.txt");
        symlink(&absent, &dangling).unwrap();
        let linked = directory.join("00000000000000000101.m01save");
        symlink(&target, &linked).unwrap();
        let created = backup_at(&save, "con.foo", 1, &[9], 100).unwrap();
        assert_eq!(created.file_name().unwrap(), "00000000000000000102.m01save");
        for stamp in 200..212 {
            backup_at(&save, "con.foo", 1, &[], stamp).unwrap();
        }
        assert!(
            fs::symlink_metadata(&dangling)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(&linked)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!absent.exists());
        assert_eq!(fs::read(target).unwrap(), b"never touch this");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_save_container_round_trips_and_checks_identity_and_checksum() {
        assert_eq!(checksum(b"123456789"), "cbf43926");
        let encoded = export("example-cart", 7, &[0, 16, 255]).unwrap();
        assert_eq!(
            import("example-cart", &encoded).unwrap(),
            (7, vec![0, 16, 255])
        );
        assert!(import("different-cart", &encoded).is_err());
        let changed = String::from_utf8(encoded)
            .unwrap()
            .replace("0010ff", "0010fe");
        assert!(import("example-cart", changed.as_bytes()).is_err());
    }
}
