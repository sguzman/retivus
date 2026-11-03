//! An explicit, offline, no-clobber safety snapshot to prepare for eventual
//! authenticated AnkiWeb sync. This does NOT perform sync or accept credentials.
//! Deliberately separate from the rest of the collection backend.
use super::{
    check_managed_media, check_regular, validate_profile_id, verify_anki_staged_copy, AnkiBackend,
    BackendResult,
};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tempfile::{Builder, NamedTempFile};
use zip::write::SimpleFileOptions;

const BUNDLE_NOTE: &str = "RETIVUS PRE-SYNC RECOVERY BUNDLE\n\
Contains a consistent SQLite collection snapshot, an Anki export with review\n\
history and referenced media, and independently copied raw media files.\n\
Contains PRIVATE study history and possibly PRIVATE media. Do not share.\n\
This is NOT proof of AnkiWeb compatibility or a complete media-sync-state\n\
backup; the media sync index is not included. Never overwrite a learner\n\
profile when recovering. Concurrent media writes can still produce an\n\
inconsistent snapshot; close other media writers before taking a backup.\n";

impl AnkiBackend {
    /// Create an owner-requested recovery artifact **without contacting a
    /// server**, opening an external Anki profile, or overwriting any file.
    ///
    /// The ZIP contains an Anki-validated SQLite backup, a complete-history
    /// .apkg for interoperable restore, and even unreferenced regular media.
    /// Detected media size/mtime changes during copying reject the archive;
    /// concurrent edits that preserve both cannot be reliably detected.
    /// The caller must choose an absolute .zip path outside its profile.
    pub fn create_pre_sync_recovery_bundle(&mut self, destination: &Path) -> BackendResult<()> {
        let managed = self
            .managed_dir
            .as_ref()
            .ok_or("pre-sync recovery requires an explicitly managed on-disk profile")?
            .clone();
        check_regular(&managed, true)?;
        check_regular(&managed.join("collection.anki2"), false)?;
        let (media, _) = check_managed_media(&managed)?;
        // Refuse symlinked/special media recursively BEFORE asking upstream
        // Anki to produce the history-preserving package.
        let media_before = snapshot_media_tree(&media, &media)?;

        if !destination.is_absolute()
            || destination.extension().and_then(|x| x.to_str()) != Some("zip")
        {
            return Err("recovery destination must be an absolute .zip path".into());
        }
        let filename = destination
            .file_name()
            .ok_or("recovery filename is required")?;
        let parent_arg = destination.parent().ok_or("recovery parent is required")?;
        check_regular(parent_arg, true)?;
        let parent = fs::canonicalize(parent_arg)?;
        if parent.starts_with(&managed) {
            return Err("recovery archive must be outside the managed profile".into());
        }
        let output = parent.join(filename);
        if fs::symlink_metadata(&output).is_ok() {
            return Err("recovery file already exists; refusing overwrite".into());
        }

        let staging = Builder::new()
            .prefix(".retivus-pre-sync-")
            .tempdir_in(&parent)?;
        let database = staging.path().join("collection.anki2");
        let apkg = staging.path().join("study-with-history.apkg");
        self.backup_to(&database)?;
        // Explicitly preserve scheduling/history: the ordinary shareable
        // .apkg export deliberately omits personal review records.
        self.export_apkg_to_with_scheduling(&apkg, true)?;

        let mut output_file = NamedTempFile::new_in(&parent)?;
        {
            let mut zip = zip::ZipWriter::new(output_file.as_file_mut());
            write_text(&mut zip, "RECOVERY-README.txt", BUNDLE_NOTE)?;
            copy_file(&mut zip, "collection.anki2", &database)?;
            copy_file(&mut zip, "study-with-history.apkg", &apkg)?;
            if fs::symlink_metadata(&media).is_ok() {
                append_media(&mut zip, &media, &media)?;
            }
            zip.finish()?;
        }
        // Media may have changed while Anki exported the .apkg or while the
        // raw files were copied. Refuse to publish an incomplete mixed-time
        // bundle if an addition, deletion, rename or metadata change occurred.
        if snapshot_media_tree(&media, &media)? != media_before {
            return Err("media tree changed during pre-sync recovery snapshot".into());
        }
        // Re-read every ZIP entry before publishing. This verifies its actual
        // on-disk CRC and catches incomplete data or archive structure, while
        // keeping the private staging file out of the destination namespace.
        {
            let mut verified = zip::ZipArchive::new(output_file.reopen()?)?;
            for index in 0..verified.len() {
                io::copy(&mut verified.by_index(index)?, &mut io::sink())?;
            }
        }
        output_file.as_file().sync_all()?;
        let finished = output_file.persist_noclobber(&output)?;
        finished.sync_all()?;
        File::open(&parent)?.sync_all()?;
        Ok(())
    }
    /// Restore a private pre-sync bundle to a NEW Retivus-owned profile.
    ///
    /// Validates all archive names and sizes before extraction, and verifies
    /// the detached SQLite copy with Anki's engine. The entire staged profile
    /// is published by same-filesystem directory rename only after all checks
    /// pass. It never changes an existing profile or accesses AnkiWeb.
    pub fn restore_pre_sync_recovery_as_new_profile(
        root: &Path,
        profile_id: &str,
        bundle: &Path,
    ) -> BackendResult<Self> {
        validate_profile_id(profile_id)?;
        if !bundle.is_absolute() || bundle.extension().and_then(|ext| ext.to_str()) != Some("zip") {
            return Err("recovery input must be an absolute .zip path".into());
        }
        check_regular(bundle, false)?;
        let root = fs::canonicalize(root)?;
        check_regular(&root, true)?;
        let target = root.join(profile_id);
        if fs::symlink_metadata(&target).is_ok() {
            return Err("recovery target already exists; refusing overwrite".into());
        }

        // The zip crate's high-level map may hide raw duplicate filenames.
        // Inspect every actual central-directory member independently first.
        let raw_names = audit_recovery_central_directory(bundle)?;
        let mut archive = zip::ZipArchive::new(File::open(bundle)?)?;
        if archive.len() != raw_names.len() {
            return Err("ZIP reader omitted or merged recovery members".into());
        }
        if archive.len() > MAX_RECOVERY_ENTRIES {
            return Err("recovery archive contains too many entries".into());
        }
        let mut names = HashSet::new();
        let mut total_size = 0_u64;
        for index in 0..archive.len() {
            let member = archive.by_index(index)?;
            let name = member.name()?.to_string();
            if member.is_dir()
                || member.unix_mode().is_some_and(|mode| {
                    let kind = mode & 0o170000;
                    kind != 0 && kind != 0o100000
                })
                || !valid_recovery_member_name(&name)
                || !names.insert(name)
            {
                return Err("invalid, duplicate or unsafe recovery ZIP member".into());
            }
            total_size = total_size
                .checked_add(member.size())
                .ok_or("recovery archive size overflow")?;
            if total_size > MAX_RECOVERY_UNCOMPRESSED_BYTES {
                return Err("recovery archive exceeds safe extraction size".into());
            }
        }
        if names != raw_names {
            return Err("ZIP reader member list disagrees with raw central directory".into());
        }
        for required in [
            "RECOVERY-README.txt",
            "collection.anki2",
            "study-with-history.apkg",
        ] {
            if !names.contains(required) {
                return Err("recovery archive lacks a required member".into());
            }
        }

        let staged = Builder::new()
            .prefix(".retivus-recover-")
            .tempdir_in(&root)?;
        let sqlite = staged.path().join("collection.anki2");
        let exported = staged.path().join(".verify-history.apkg");
        for index in 0..archive.len() {
            let mut member = archive.by_index(index)?;
            let name = member.name()?.into_owned();
            if name == "RECOVERY-README.txt" {
                // Verify CRC even for an informational member.
                let bytes = io::copy(&mut member, &mut io::sink())?;
                if bytes != member.size() {
                    return Err("short read from recovery note".into());
                }
                continue;
            }
            let path = match name.as_str() {
                "collection.anki2" => sqlite.clone(),
                "study-with-history.apkg" => exported.clone(),
                _ => staged.path().join(&name),
            };
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            // TempDir is private (0700); no archive path may escape it.
            let mut output = File::create_new(&path)?;
            let bytes = io::copy(&mut member, &mut output)?;
            if bytes != member.size() {
                return Err("incomplete recovery ZIP member".into());
            }
            output.sync_all()?;
        }

        verify_anki_staged_copy(&sqlite)?;
        let other_archive = zip::ZipArchive::new(File::open(&exported)?)?;
        if other_archive.len() == 0 {
            return Err("Anki history package is empty".into());
        }
        drop(other_archive);
        fs::remove_file(&exported)?;
        File::open(staged.path())?.sync_all()?;
        // Reserve the name with create_dir first; this operation fails if a
        // different profile appeared while we were validating the archive.
        fs::create_dir(&target)?;
        if let Err(err) = fs::rename(staged.path(), &target) {
            let _ = fs::remove_dir(&target); // only if still empty
            return Err(err.into());
        }
        File::open(&root)?.sync_all()?;
        Self::open_managed_profile(&root, profile_id)
    }
}

const MAX_RECOVERY_ENTRIES: usize = 200_000;
const MAX_RECOVERY_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MAX_RECOVERY_CENTRAL_DIRECTORY_BYTES: u64 = 256 * 1024 * 1024;

fn u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// Audit raw central-directory entries instead of trusting a ZIP reader that
/// can index two raw filenames under one key. Only the canonical, single-disk
/// ZIP layouts written by the pinned Retivus ZIP writer are accepted, with
/// ZIP64 supported for large media collections.
fn audit_recovery_central_directory(path: &Path) -> BackendResult<HashSet<String>> {
    let mut file = File::open(path)?;
    let total_length = file.metadata()?.len();
    if total_length < 22 {
        return Err("recovery ZIP is too short".into());
    }
    let tail_length = total_length.min(65_557) as usize;
    file.seek(SeekFrom::End(-(tail_length as i64)))?;
    let mut tail = vec![0_u8; tail_length];
    file.read_exact(&mut tail)?;
    let eocd_index = (0..=tail_length - 22).rev().find(|&i| {
        tail[i..].starts_with(b"PK\x05\x06")
            && usize::from(u16_le(&tail, i + 20)) == tail_length - i - 22
    });
    let index = eocd_index.ok_or("recovery ZIP end-of-central-directory missing")?;
    let end_record = &tail[index..index + 22];
    if u16_le(end_record, 4) != 0 || u16_le(end_record, 6) != 0 {
        return Err("multi-disk recovery ZIP is unsupported".into());
    }
    let eocd_offset = total_length - tail_length as u64 + index as u64;
    let mut count = u16_le(end_record, 10) as u64;
    let mut central_size = u32_le(end_record, 12) as u64;
    let mut central_start = u32_le(end_record, 16) as u64;
    if count == u16::MAX as u64
        || central_size == u32::MAX as u64
        || central_start == u32::MAX as u64
    {
        if eocd_offset < 20 {
            return Err("ZIP64 locator is missing".into());
        }
        file.seek(SeekFrom::Start(eocd_offset - 20))?;
        let mut locator = [0_u8; 20];
        file.read_exact(&mut locator)?;
        if &locator[..4] != b"PK\x06\x07" || u32_le(&locator, 4) != 0 || u32_le(&locator, 16) != 1 {
            return Err("invalid or multi-disk ZIP64 locator".into());
        }
        file.seek(SeekFrom::Start(u64_le(&locator, 8)))?;
        let mut zip64 = [0_u8; 56];
        file.read_exact(&mut zip64)?;
        if &zip64[..4] != b"PK\x06\x06" || u32_le(&zip64, 16) != 0 || u32_le(&zip64, 20) != 0 {
            return Err("invalid or multi-disk ZIP64 central directory".into());
        }
        count = u64_le(&zip64, 32);
        central_size = u64_le(&zip64, 40);
        central_start = u64_le(&zip64, 48);
    }
    if count > MAX_RECOVERY_ENTRIES as u64 || central_size > MAX_RECOVERY_CENTRAL_DIRECTORY_BYTES {
        return Err("recovery ZIP central directory exceeds bounded limits".into());
    }
    let end = central_start
        .checked_add(central_size)
        .ok_or("invalid central directory length")?;
    if end > eocd_offset {
        return Err("recovery ZIP central directory points beyond its end".into());
    }
    file.seek(SeekFrom::Start(central_start))?;
    let mut names = HashSet::new();
    for _ in 0..count {
        if file.stream_position()?.saturating_add(46) > end {
            return Err("truncated recovery ZIP central directory".into());
        }
        let mut record = [0_u8; 46];
        file.read_exact(&mut record)?;
        if &record[..4] != b"PK\x01\x02" {
            return Err("malformed recovery ZIP central entry".into());
        }
        let name_length = u16_le(&record, 28) as usize;
        let skip = u64::from(u16_le(&record, 30)) + u64::from(u16_le(&record, 32));
        if file
            .stream_position()?
            .checked_add(name_length as u64 + skip)
            .is_none_or(|position| position > end)
        {
            return Err("recovery ZIP member metadata crosses central directory".into());
        }
        let mut bytes = vec![0_u8; name_length];
        file.read_exact(&mut bytes)?;
        let name = String::from_utf8(bytes)?;
        if !valid_recovery_member_name(&name) || !names.insert(name) {
            return Err("unsafe or duplicate raw ZIP central filename".into());
        }
        file.seek(SeekFrom::Current(skip as i64))?;
    }
    if file.stream_position()? != end {
        return Err("extra or mismatched recovery ZIP central metadata".into());
    }
    Ok(names)
}

fn valid_recovery_member_name(name: &str) -> bool {
    if matches!(
        name,
        "RECOVERY-README.txt" | "collection.anki2" | "study-with-history.apkg"
    ) {
        return true;
    }
    let Some(relative) = name.strip_prefix("collection.media/") else {
        return false;
    };
    !relative.is_empty()
        && relative.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.contains('\\')
                && !part.contains('\0')
        })
}

type BundleZip<'a> = zip::ZipWriter<&'a mut File>;

fn write_text(zip: &mut BundleZip<'_>, name: &str, text: &str) -> BackendResult<()> {
    zip.start_file(name, SimpleFileOptions::default())?;
    io::copy(&mut text.as_bytes(), zip)?;
    Ok(())
}

fn copy_file(zip: &mut BundleZip<'_>, name: &str, path: &Path) -> BackendResult<()> {
    check_regular(path, false)?;
    let before = fs::metadata(path)?;
    let mut input = File::open(path)?;
    zip.start_file(name, SimpleFileOptions::default())?;
    let size = io::copy(&mut input, zip)?;
    let after = fs::symlink_metadata(path)?;
    if !after.file_type().is_file()
        || before.len() != size
        || before.len() != after.len()
        || before.modified()? != after.modified()?
    {
        return Err("a recovery source file changed while being copied".into());
    }
    Ok(())
}

fn media_entries(dir: &Path) -> BackendResult<Vec<PathBuf>> {
    let mut children = fs::read_dir(dir)?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();
    Ok(children)
}

#[derive(Debug, PartialEq, Eq)]
struct MediaStamp {
    size: u64,
    modified: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
}

/// Inventory the *whole* media tree, not just the files that happened to be
/// visited by the ZIP writer. A second inventory catches files added, removed,
/// renamed or replaced while the collection and package exports were running.
fn snapshot_media_tree(root: &Path, dir: &Path) -> BackendResult<Vec<(PathBuf, MediaStamp)>> {
    let mut snapshot = Vec::new();
    collect_media_snapshot(root, dir, &mut snapshot)?;
    Ok(snapshot)
}

fn collect_media_snapshot(
    root: &Path,
    dir: &Path,
    snapshot: &mut Vec<(PathBuf, MediaStamp)>,
) -> BackendResult<()> {
    match fs::symlink_metadata(dir) {
        Err(err) if err.kind() == io::ErrorKind::NotFound && dir == root => return Ok(()),
        Err(err) => return Err(err.into()),
        Ok(meta) if !meta.file_type().is_dir() => return Err("unsafe media directory".into()),
        Ok(_) => {}
    }
    for path in media_entries(dir)? {
        let relative = path.strip_prefix(root)?;
        if relative.to_str().is_none() {
            return Err("non-UTF8 media name needs manual recovery".into());
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_dir() {
            collect_media_snapshot(root, &path, snapshot)?;
        } else if meta.file_type().is_file() {
            #[cfg(unix)]
            use std::os::unix::fs::MetadataExt;
            snapshot.push((
                relative.to_path_buf(),
                MediaStamp {
                    size: meta.len(),
                    modified: meta.modified()?,
                    #[cfg(unix)]
                    device: meta.dev(),
                    #[cfg(unix)]
                    inode: meta.ino(),
                    #[cfg(unix)]
                    changed_seconds: meta.ctime(),
                    #[cfg(unix)]
                    changed_nanoseconds: meta.ctime_nsec(),
                },
            ));
        } else {
            return Err("refusing symlink or special file inside media tree".into());
        }
    }
    Ok(())
}

fn append_media(zip: &mut BundleZip<'_>, root: &Path, dir: &Path) -> BackendResult<()> {
    check_regular(dir, true)?;
    for path in media_entries(dir)? {
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_dir() {
            append_media(zip, root, &path)?;
        } else if meta.file_type().is_file() {
            let relative = path.strip_prefix(root)?;
            let filename = relative.to_str().ok_or("non-UTF8 media path")?;
            copy_file(zip, &format!("collection.media/{filename}"), &path)?;
        } else {
            return Err("media changed to a symlink or special file during snapshot".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn pre_sync_archive_is_private_offline_no_clobber_and_includes_unreferenced_media() {
        let dir = tempdir().unwrap();
        let mut profile = AnkiBackend::create_managed_profile(dir.path(), "safe").unwrap();
        let deck = profile.create_deck("Recovery").unwrap();
        profile.add_basic_note(deck, "cielo", "sky").unwrap();
        let initial = profile.collection_stats().unwrap();
        let media = dir.path().join("safe/collection.media/extra");
        fs::create_dir_all(&media).unwrap();
        fs::write(
            media.join("private-unreferenced.bin"),
            b"private synthetic media",
        )
        .unwrap();

        let bundle = dir.path().join("offline-pre-sync.zip");
        profile.create_pre_sync_recovery_bundle(&bundle).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&bundle).unwrap().permissions().mode() & 0o077,
                0,
                "recovery ZIP must not be readable by group or others"
            );
        }
        let mut archive = zip::ZipArchive::new(File::open(&bundle).unwrap()).unwrap();
        assert!(archive.by_name("RECOVERY-README.txt").is_ok());
        let mut bytes = Vec::new();
        archive
            .by_name("collection.anki2")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"));
        bytes.clear();
        archive
            .by_name("study-with-history.apkg")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.starts_with(b"PK"));
        bytes.clear();
        archive
            .by_name("collection.media/extra/private-unreferenced.bin")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"private synthetic media");
        assert_eq!(profile.collection_stats().unwrap(), initial);
        assert!(profile.create_pre_sync_recovery_bundle(&bundle).is_err());
        assert!(profile
            .create_pre_sync_recovery_bundle(Path::new("relative.zip"))
            .is_err());

        let mut transient = AnkiBackend::in_memory().unwrap();
        assert!(transient
            .create_pre_sync_recovery_bundle(&dir.path().join("invalid.zip"))
            .is_err());
        assert!(!dir.path().join("invalid.zip").exists());
    }

    #[test]
    fn private_recovery_round_trip_preserves_review_history_and_unreferenced_media() {
        let dir = tempdir().unwrap();
        let mut profile = AnkiBackend::create_managed_profile(dir.path(), "original").unwrap();
        let deck = profile.create_deck("Recovery").unwrap();
        profile.add_basic_note(deck, "luz", "light").unwrap();
        profile.select_review_deck(deck).unwrap();
        profile.grade_next_card(4).unwrap().unwrap();
        let expected = profile.collection_stats().unwrap();
        assert_eq!(expected.review_events, 1);
        let media = dir.path().join("original/collection.media/nested");
        fs::create_dir_all(&media).unwrap();
        fs::write(media.join("private.dat"), b"raw-only evidence").unwrap();

        let bundle = dir.path().join("pre-sync.zip");
        profile.create_pre_sync_recovery_bundle(&bundle).unwrap();
        let recovered =
            AnkiBackend::restore_pre_sync_recovery_as_new_profile(dir.path(), "recovered", &bundle)
                .unwrap();
        assert_eq!(recovered.collection_stats().unwrap(), expected);
        assert_eq!(
            fs::read(
                dir.path()
                    .join("recovered/collection.media/nested/private.dat")
            )
            .unwrap(),
            b"raw-only evidence"
        );
        assert!(AnkiBackend::restore_pre_sync_recovery_as_new_profile(
            dir.path(),
            "recovered",
            &bundle
        )
        .is_err());
        assert_eq!(recovered.collection_stats().unwrap(), expected);
        recovered.close().unwrap();
        let reopened = AnkiBackend::open_managed_profile(dir.path(), "recovered").unwrap();
        assert_eq!(reopened.collection_stats().unwrap(), expected);
    }

    #[test]
    fn malicious_pre_sync_zip_is_rejected_before_creating_any_profile() {
        use std::io::Write;
        let dir = tempdir().unwrap();
        let mut profile = AnkiBackend::create_managed_profile(dir.path(), "source").unwrap();
        profile.add_basic_note(1, "a", "b").unwrap();
        let base = dir.path().join("good.zip");
        profile.create_pre_sync_recovery_bundle(&base).unwrap();

        for (index, unsafe_name) in [
            "collection.media/../escape.txt",
            "collection.media//duplicate.txt",
            "collection.media/back\\\\slash.txt",
            "unknown.txt",
            "collection.media/./dot.txt",
        ]
        .iter()
        .enumerate()
        {
            let bad = dir.path().join(format!("bad-{index}.zip"));
            let mut original = zip::ZipArchive::new(File::open(&base).unwrap()).unwrap();
            let mut writer = zip::ZipWriter::new(File::create(&bad).unwrap());
            for idx in 0..original.len() {
                let mut entry = original.by_index(idx).unwrap();
                writer
                    .start_file(entry.name().unwrap(), SimpleFileOptions::default())
                    .unwrap();
                io::copy(&mut entry, &mut writer).unwrap();
            }
            writer
                .start_file(*unsafe_name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"untrusted data").unwrap();
            writer.finish().unwrap();
            let new_name = format!("reject-{index}");
            assert!(
                AnkiBackend::restore_pre_sync_recovery_as_new_profile(dir.path(), &new_name, &bad)
                    .is_err(),
                "rejected unsafe archive member {unsafe_name}"
            );
            assert!(!dir.path().join(new_name).exists());
        }
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    fn duplicate_recovery_members_are_rejected_before_profile_creation() {
        use std::io::Write;
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "source").unwrap();
        backend.add_basic_note(1, "front", "back").unwrap();
        let original = dir.path().join("original.zip");
        backend.create_pre_sync_recovery_bundle(&original).unwrap();

        // zip 9's writer correctly refuses duplicate names, so construct
        // a malicious raw ZIP by giving an extra entry a same-length name
        // first, then mutating its local and central-directory filename.
        let tampered = dir.path().join("duplicate.zip");
        let mut reader = zip::ZipArchive::new(File::open(&original).unwrap()).unwrap();
        let mut writer = zip::ZipWriter::new(File::create(&tampered).unwrap());
        for idx in 0..reader.len() {
            let mut member = reader.by_index(idx).unwrap();
            writer
                .start_file(member.name().unwrap(), SimpleFileOptions::default())
                .unwrap();
            io::copy(&mut member, &mut writer).unwrap();
        }
        writer
            .start_file("RECOVERY-README.txz", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"synthetic collision").unwrap();
        writer.finish().unwrap();
        let mut bytes = fs::read(&tampered).unwrap();
        let needle = b"RECOVERY-README.txz";
        let replacement = b"RECOVERY-README.txt";
        assert_eq!(needle.len(), replacement.len());
        let positions: Vec<_> = bytes
            .windows(needle.len())
            .enumerate()
            .filter_map(|(index, window)| (window == needle).then_some(index))
            .collect();
        assert_eq!(positions.len(), 2);
        for position in positions {
            bytes[position..position + replacement.len()].copy_from_slice(replacement);
        }
        fs::write(&tampered, bytes).unwrap();
        assert!(AnkiBackend::restore_pre_sync_recovery_as_new_profile(
            dir.path(),
            "must-not-exist",
            &tampered
        )
        .is_err());
        assert!(!dir.path().join("must-not-exist").exists());
    }

    #[test]
    fn media_inventory_detects_additions_removals_and_renames() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("collection.media");
        fs::create_dir(&media).unwrap();
        fs::write(media.join("one.dat"), b"one").unwrap();
        let original = snapshot_media_tree(&media, &media).unwrap();
        fs::write(media.join("two.dat"), b"two").unwrap();
        assert_ne!(snapshot_media_tree(&media, &media).unwrap(), original);
        fs::remove_file(media.join("two.dat")).unwrap();
        assert_eq!(snapshot_media_tree(&media, &media).unwrap(), original);
        fs::rename(media.join("one.dat"), media.join("renamed.dat")).unwrap();
        assert_ne!(snapshot_media_tree(&media, &media).unwrap(), original);
        fs::remove_file(media.join("renamed.dat")).unwrap();
        assert_ne!(snapshot_media_tree(&media, &media).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn media_inventory_refuses_non_utf8_directory_names() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let dir = tempdir().unwrap();
        let media = dir.path().join("collection.media");
        fs::create_dir(&media).unwrap();
        fs::create_dir(media.join(OsString::from_vec(vec![0xff]))).unwrap();
        assert!(snapshot_media_tree(&media, &media).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn pre_sync_archive_refuses_media_symlinks_without_leaking_contents() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let mut profile = AnkiBackend::create_managed_profile(dir.path(), "safe").unwrap();
        let media = dir.path().join("safe/collection.media");
        fs::create_dir_all(&media).unwrap();
        let outside = dir.path().join("private-outside.txt");
        fs::write(&outside, b"must never be archived").unwrap();
        symlink(&outside, media.join("not-owned.txt")).unwrap();
        let out = dir.path().join("blocked.zip");
        assert!(profile.create_pre_sync_recovery_bundle(&out).is_err());
        assert!(!out.exists());
    }
}
