//! Explicit, recovery-gated media synchronization via Anki's pinned Rust engine.
//! Kept separate from collection sync and inaccessible to automatic GUI actions.
use super::sync_login::{obtain_temporary_host_key, validate_login_input};
use super::sync_preflight::new_ankiweb_http_client;
use super::{AnkiBackend, BackendResult};
use anki::sync::login::SyncAuth;
use std::path::Path;

impl AnkiBackend {
    /// May upload, download or delete media. Requires distinct explicit approval
    /// and a fresh private recovery ZIP before contacting AnkiWeb.
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
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn unapproved_media_sync_does_not_create_an_index_or_recovery_zip() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "owned").unwrap();
        let backup = root.path().join("never-created.zip");
        let index = root.path().join("owned/collection.mdb");
        let before = backend.collection_stats().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(runtime
            .block_on(backend.sync_ankiweb_media_with_recovery(
                "user@example.test",
                "synthetic-password",
                &backup,
                false,
            ))
            .is_err());
        assert!(!backup.exists());
        assert!(!index.exists());
        assert_eq!(backend.collection_stats().unwrap(), before);
    }
}
