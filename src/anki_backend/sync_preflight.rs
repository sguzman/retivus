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
        self.query_remote_status_with_auth(auth, client).await
    }

    async fn query_remote_status_with_auth(
        &mut self,
        auth: SyncAuth,
        client: reqwest::Client,
    ) -> BackendResult<RemoteSyncPreflight> {
        let local = self.collection.sync_meta()?;
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
    fn synthetic_remote_status_is_read_only_and_preserves_local_study_data() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "local").unwrap();
        let deck = backend.create_deck("No destructive sync").unwrap();
        backend.add_basic_note(deck, "pregunta", "answer").unwrap();
        let before = backend.collection_stats().unwrap();

        // The response uses Anki's own sync-meta serialization and wire
        // compression. It never connects to AnkiWeb or a user account.
        let json = serde_json::to_vec(&backend.collection.sync_meta().unwrap()).unwrap();
        let compressed = zstd::stream::encode_all(json.as_slice(), 1).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0_u8; 4096];
            loop {
                let size = stream.read(&mut buf).unwrap();
                assert!(size > 0, "incomplete synthetic status request");
                request.extend_from_slice(&buf[..size]);
                if let Some(header_end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..header_end]);
                    let body = &request[header_end + 4..];
                    if header
                        .to_ascii_lowercase()
                        .contains("transfer-encoding: chunked")
                    {
                        if body.windows(5).any(|b| b == b"0\r\n\r\n") {
                            break;
                        }
                    } else if let Some(len) = header.lines().find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    }) {
                        if body.len() >= len {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                assert!(request.len() < 1024 * 1024, "oversized status request");
            }
            assert!(
                String::from_utf8_lossy(&request).starts_with("POST /sync/meta "),
                "only the metadata endpoint may be called"
            );
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                json.len(),
                compressed.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(&compressed).unwrap();
            stream.flush().unwrap();
        });

        let auth = SyncAuth {
            hkey: "synthetic-test-key".into(),
            endpoint: Some(reqwest::Url::parse(&endpoint).unwrap()),
            io_timeout_secs: Some(10),
        };
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime
            .block_on(backend.query_remote_status_with_auth(auth, client))
            .unwrap();
        server.join().unwrap();
        assert_eq!(result.required, RemoteSyncNeed::NoChanges);
        assert_eq!(backend.collection_stats().unwrap(), before);
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
