//! Explicit, read-only AnkiWeb collection sync status via the pinned Anki engine.
//! This never performs collection sync, media sync, login, or credential storage.
//! A caller must supply a temporary AnkiWeb host key to issue the status request.
use super::{check_managed_media, check_regular, AnkiBackend, BackendResult};
use anki::sync::collection::normal::{ClientSyncState, SyncActionRequired};
use anki::sync::collection::status::online_sync_status_check;
use anki::sync::http_client::HttpSyncClient;
use anki::sync::login::SyncAuth;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteSyncNeed {
    NoChanges,
    NormalSync,
    FullSync {
        upload_possible: bool,
        download_possible: bool,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct RemoteSyncPreflight {
    pub required: RemoteSyncNeed,
    pub server_message: String,
    pub host_number: u32,
    /// Only informational. Never automatically follow or persist this value.
    pub suggested_endpoint: Option<String>,
}

impl From<SyncActionRequired> for RemoteSyncNeed {
    fn from(required: SyncActionRequired) -> Self {
        match required {
            SyncActionRequired::NoChanges => Self::NoChanges,
            SyncActionRequired::NormalSyncRequired => Self::NormalSync,
            SyncActionRequired::FullSyncRequired {
                upload_ok,
                download_ok,
            } => Self::FullSync {
                upload_possible: upload_ok,
                download_possible: download_ok,
            },
        }
    }
}

impl From<ClientSyncState> for RemoteSyncPreflight {
    fn from(status: ClientSyncState) -> Self {
        Self {
            required: status.required.into(),
            server_message: status.server_message,
            host_number: status.host_number,
            suggested_endpoint: status.new_endpoint,
        }
    }
}

fn validate_temporary_host_key(key: &str) -> BackendResult<()> {
    // Anki's host keys are ASCII tokens. Reject control characters, whitespace
    // and excessive lengths instead of accepting log/header injection inputs.
    if key.is_empty() || key.len() > 1024 || key.bytes().any(|byte| !byte.is_ascii_graphic()) {
        return Err("a nonempty AnkiWeb host-key token is required".into());
    }
    Ok(())
}

impl AnkiBackend {
    /// Query the upstream collection-sync status without exchanging any cards.
    ///
    /// A preflight may report FullSync; it NEVER chooses upload/download or
    /// touches a remote collection. It does not handle media, credentials or
    /// login. The host key remains in this call and is not written to disk.
    /// Invoking this explicitly opts into one network request to AnkiWeb.
    pub async fn inspect_ankiweb_sync_status(
        &mut self,
        temporary_host_key: &str,
    ) -> BackendResult<RemoteSyncPreflight> {
        let managed = self
            .managed_dir
            .as_ref()
            .ok_or("remote sync status requires a Retivus-managed disk profile")?;
        check_regular(managed, true)?;
        check_regular(&managed.join("collection.anki2"), false)?;
        check_managed_media(managed)?;
        validate_temporary_host_key(temporary_host_key)?;

        let local = self.collection.sync_meta()?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()?;
        let auth = SyncAuth {
            hkey: temporary_host_key.to_owned(),
            endpoint: None,
            io_timeout_secs: Some(30),
        };
        let mut upstream = HttpSyncClient::new(auth, client);
        let status = online_sync_status_check(local, &mut upstream).await?;
        Ok(status.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn all_upstream_sync_statuses_have_explicit_non_destructive_outcomes() {
        assert_eq!(
            RemoteSyncNeed::from(SyncActionRequired::NoChanges),
            RemoteSyncNeed::NoChanges
        );
        assert_eq!(
            RemoteSyncNeed::from(SyncActionRequired::NormalSyncRequired),
            RemoteSyncNeed::NormalSync
        );
        assert_eq!(
            RemoteSyncNeed::from(SyncActionRequired::FullSyncRequired {
                upload_ok: false,
                download_ok: true,
            }),
            RemoteSyncNeed::FullSync {
                upload_possible: false,
                download_possible: true,
            }
        );
        assert_eq!(
            RemoteSyncNeed::from(SyncActionRequired::FullSyncRequired {
                upload_ok: true,
                download_ok: false,
            }),
            RemoteSyncNeed::FullSync {
                upload_possible: true,
                download_possible: false,
            }
        );
    }

    #[test]
    fn invalid_host_keys_fail_before_any_network_request() {
        for value in ["", "a b", "a\nkey", " a", "a\tkey"] {
            assert!(validate_temporary_host_key(value).is_err());
        }
        assert!(validate_temporary_host_key(&"a".repeat(1025)).is_err());
        assert!(validate_temporary_host_key("temporary-host-key").is_ok());
    }

    #[test]
    fn preflight_never_opens_unmanaged_collections_or_accepts_blank_keys() {
        let mut memory = AnkiBackend::in_memory().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(runtime
            .block_on(memory.inspect_ankiweb_sync_status("synthetic-key"))
            .is_err());

        let root = tempdir().unwrap();
        let mut managed = AnkiBackend::create_managed_profile(root.path(), "local").unwrap();
        let deck = managed.create_deck("Before preflight").unwrap();
        managed.add_basic_note(deck, "front", "back").unwrap();
        let before = managed.collection_stats().unwrap();
        assert!(runtime
            .block_on(managed.inspect_ankiweb_sync_status(""))
            .is_err());
        assert_eq!(managed.collection_stats().unwrap(), before);
    }
}
