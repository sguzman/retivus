//! An explicit, offline, no-clobber safety snapshot to prepare for eventual
//! authenticated AnkiWeb sync. This does NOT perform sync or accept credentials.
//! Deliberately separate from the rest of the collection backend.
use super::{check_managed_media, check_regular, AnkiBackend, BackendResult};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tempfile::{Builder, NamedTempFile};
use zip::write::SimpleFileOptions;

const BUNDLE_NOTE: &str = "RETIVUS PRE-SYNC RECOVERY BUNDLE\n\
Contains a consistent SQLite collection snapshot, an Anki export with review\n\
history and referenced media, and independently copied raw media files.\n\
Contains PRIVATE study history and possibly PRIVATE media. Do not share.\n\
This is NOT proof of AnkiWeb compatibility or a complete media-sync-state\n\
backup; the media sync index is not included. Never overwrite a learner\n\
profile when recovering. All tests use disposable original fixtures.\n";

impl AnkiBackend {
    /// Create an owner-requested recovery artifact **without contacting a
    /// server**, opening an external Anki profile, or overwriting any file.
    ///
    /// The ZIP contains an Anki-validated SQLite backup, a complete-history
    /// .apkg for interoperable restore, and even unreferenced regular media.
    /// Source-media changes during copying cause the archive to be rejected.
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
        verify_media_tree(&media, &media)?;

        if !destination.is_absolute() || destination.extension().and_then(|x| x.to_str()) != Some("zip") {
            return Err("recovery destination must be an absolute .zip path".into());
        }
        let filename = destination.file_name().ok_or("recovery filename is required")?;
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

        let staging = Builder::new().prefix(".retivus-pre-sync-").tempdir_in(&parent)?;
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
        output_file.as_file().sync_all()?;
        let finished = output_file.persist_noclobber(&output)?;
        finished.sync_all()?;
        File::open(&parent)?.sync_all()?;
        Ok(())
    }
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
    if !after.file_type().is_file() || before.len() != size || before.len() != after.len()
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

fn verify_media_tree(root: &Path, dir: &Path) -> BackendResult<()> {
    match fs::symlink_metadata(dir) {
        Err(err) if err.kind() == io::ErrorKind::NotFound && dir == root => return Ok(()),
        Err(err) => return Err(err.into()),
        Ok(meta) if !meta.file_type().is_dir() => return Err("unsafe media directory".into()),
        Ok(_) => {}
    }
    for path in media_entries(dir)? {
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_dir() {
            verify_media_tree(root, &path)?;
        } else if !meta.file_type().is_file() {
            return Err("refusing symlink or special file inside media tree".into());
        }
        if path.strip_prefix(root)?.to_str().is_none() {
            return Err("non-UTF8 media name needs manual recovery".into());
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
        fs::write(media.join("private-unreferenced.bin"), b"private synthetic media").unwrap();

        let bundle = dir.path().join("offline-pre-sync.zip");
        profile.create_pre_sync_recovery_bundle(&bundle).unwrap();
        let mut archive = zip::ZipArchive::new(File::open(&bundle).unwrap()).unwrap();
        assert!(archive.by_name("RECOVERY-README.txt").is_ok());
        let mut bytes = Vec::new();
        archive.by_name("collection.anki2").unwrap().read_to_end(&mut bytes).unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"));
        bytes.clear();
        archive.by_name("study-with-history.apkg").unwrap().read_to_end(&mut bytes).unwrap();
        assert!(bytes.starts_with(b"PK"));
        bytes.clear();
        archive.by_name("collection.media/extra/private-unreferenced.bin")
            .unwrap().read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"private synthetic media");
        assert_eq!(profile.collection_stats().unwrap(), initial);
        assert!(profile.create_pre_sync_recovery_bundle(&bundle).is_err());
        assert!(profile.create_pre_sync_recovery_bundle(Path::new("relative.zip")).is_err());

        let mut transient = AnkiBackend::in_memory().unwrap();
        assert!(transient.create_pre_sync_recovery_bundle(&dir.path().join("invalid.zip")).is_err());
        assert!(!dir.path().join("invalid.zip").exists());
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
