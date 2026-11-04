//! Explicit, recovery-gated media synchronization via Anki's pinned Rust engine.
//! Kept separate from collection sync and inaccessible to automatic GUI actions.
use super::sync_login::{obtain_temporary_host_key, validate_login_input};
use super::sync_preflight::new_ankiweb_http_client;
use super::{AnkiBackend, BackendResult};
use anki::sync::login::SyncAuth;
use rusqlite::OpenFlags;
use std::path::Path;
use std::collections::HashMap;
use std::time::UNIX_EPOCH;

/// The pinned Anki media engine registers filesystem changes immediately
/// before network sync. Dirty flags alone can miss a file added, edited, or
/// removed outside Anki's media manager. Refuse such changes on a profile
/// with previous sync history *before* allowing that registration to write.
/// This is deliberately conservative, and never modifies the index.
fn refuse_unindexed_or_changed_media(managed: &Path, db: &rusqlite::Connection) -> BackendResult<()> {
    let mut indexed = HashMap::<String, i64>::new();
    let mut statement = db.prepare("SELECT fname, mtime FROM media WHERE csum IS NOT NULL")?;
    let entries = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    for entry in entries {
        let (filename, recorded_mtime): (String, i64) = entry?;
        indexed.insert(filename, recorded_mtime);
    }

    let media_dir = managed.join("collection.media");
    let directory = match std::fs::read_dir(&media_dir) {
        Ok(directory) => directory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if indexed.is_empty() {
                return Ok(());
            }
            return Err("media sync refused: indexed media files are missing on disk".into());
        }
        Err(error) => return Err(error.into()),
    };

    for entry in directory {
        let entry = entry?;
        // A nested directory or symlink cannot be safely interpreted as
        // one of Anki's flat, regular media files.
        if !entry.file_type()?.is_file() {
            return Err("media sync refused: unexpected non-regular media entry".into());
        }
        let name = entry.file_name();
        let name = name.to_str().ok_or("media sync refused: non-Unicode media filename")?;
        let recorded_mtime = indexed
            .remove(name)
            .ok_or("media sync refused: local media file is absent from the clean media index")?;
        // The pinned Anki change tracker records whole-second mtimes. Compare
        // using the same resolution, without altering the file or index.
        let current_mtime = i64::try_from(
            entry
                .metadata()?
                .modified()?
                .duration_since(UNIX_EPOCH)?
                .as_secs(),
        )?;
        if current_mtime != recorded_mtime {
            return Err("media sync refused: indexed media file changed on disk".into());
        }
    }
    if !indexed.is_empty() {
        return Err("media sync refused: indexed media file is missing on disk".into());
    }
    Ok(())
}

impl AnkiBackend {
    /// Inspect the existing media index *without creating or writing it*.
    /// Once a profile has previously synchronized media, any pending local
    /// media file/deletion may conflict with an unseen same-name server change.
    /// The pinned engine is remote-wins in that case. Refuse the public media
    /// sync rather than treating an offline recovery ZIP as conflict resolution.
    ///
    /// This check cannot inspect remote-only changes, prove that a filename
    /// never existed upstream, or protect the first media sync. It also
    /// conservatively rejects newly added, removed or timestamp-changed media
    /// after a prior sync. It cannot detect edits preserving whole-second mtime.
    pub fn inspect_local_media_conflict_risk(&self) -> BackendResult<()> {
        self.validate_sync_profile()?;
        let managed = self
            .managed_dir
            .as_ref()
            .ok_or("media conflict inspection requires an owned profile")?;
        let index = managed.join("collection.mdb");
        let metadata = match std::fs::symlink_metadata(&index) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if !metadata.file_type().is_file() {
            return Err("media sync refused: media index is not a regular owned file".into());
        }
        // READ_ONLY forbids silently creating or migrating the on-disk index
        // as part of an offline preflight or a refused remote operation.
        let db = rusqlite::Connection::open_with_flags(&index, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        // A missing or malformed metadata row must fail closed. An EXISTS
        // cross-join would otherwise return false for an empty meta table.
        let last_usn: i64 = db.query_row("SELECT lastUsn FROM meta", [], |row| row.get(0))?;
        if last_usn < 0 {
            return Err("media sync refused: invalid media sync sequence".into());
        }
        let has_dirty: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM media WHERE dirty != 0)",
            [],
            |row| row.get(0),
        )?;
        if last_usn > 0 {
            if has_dirty {
                return Err(
                    "AnkiWeb media sync refused: previously synchronized media has pending local changes; a remote same-name change could replace local bytes. A private recovery ZIP is not conflict-free reconciliation".into(),
                );
            }
            // Even if SQLite records are clean, a direct filesystem edit can
            // be registered as dirty only after Anki starts media sync.
            refuse_unindexed_or_changed_media(managed, &db)?;
        }
        Ok(())
    }

    /// May upload, download or delete media. Requires distinct explicit approval
    /// and a fresh private recovery ZIP before contacting AnkiWeb.
    /// The pinned Anki engine favors remote content on same-name media conflicts;
    /// server-initiated deletion also removes local clean media. The pre-sync
    /// recovery ZIP retains the original local bytes and media index.
    pub async fn sync_ankiweb_media_with_recovery(
        &mut self,
        username: &str,
        password: &str,
        recovery_zip: &Path,
        approve_media_changes: bool,
    ) -> BackendResult<()> {
        self.validate_sync_profile()?;
        validate_login_input(username, password)?;
        if !approve_media_changes {
            return Err("AnkiWeb media changes require explicit approval".into());
        }
        // This is the PUBLIC safety boundary. The private client helper is
        // only used by disposable-server protocol characterization tests.
        self.inspect_local_media_conflict_risk()?;
        let client = new_ankiweb_http_client()?;
        self.sync_ankiweb_media_with_client(username, password, recovery_zip, client, None)
            .await
    }

    async fn sync_ankiweb_media_with_client(
        &mut self,
        username: &str,
        password: &str,
        recovery_zip: &Path,
        client: reqwest::Client,
        endpoint: Option<String>,
    ) -> BackendResult<()> {
        // Initialize the owned media index so the recovery ZIP always includes
        // it; this happens only after consent and before any network traffic.
        drop(self.collection.media()?);
        self.create_pre_sync_recovery_bundle(recovery_zip)?;
        let key =
            obtain_temporary_host_key(username, password, endpoint.clone(), client.clone()).await?;
        let auth = SyncAuth {
            hkey: key,
            endpoint: match endpoint {
                Some(value) => Some(reqwest::Url::parse(&value)?),
                None => None,
            },
            io_timeout_secs: Some(30),
        };
        let manager = self.collection.media()?;
        let progress = self.collection.new_progress_handler();
        manager
            .sync_media(progress, auth, client, None)
            .await
            .map_err(|_| "AnkiWeb media sync failed; recovery ZIP retained")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
