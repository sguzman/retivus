//! An explicitly approved Anki collection sync operation.
//!
//! The caller must first provide a brand-new absolute recovery ZIP destination
//! outside the Retivus profile, then explicitly authorize collection changes.
//! Only upstream *normal sync* is attempted; a required full sync is reported
//! but NEVER performed automatically. Media sync is not implemented here.
//! This backend-only API is not connected to the GUI or any automatic job.
use super::sync_login::{obtain_temporary_host_key, validate_login_input};
use super::sync_preflight::new_ankiweb_http_client;
use super::{AnkiBackend, BackendResult};
use anki::sync::collection::normal::SyncActionRequired;
use anki::sync::login::SyncAuth;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionSyncOutcome {
    /// The pinned upstream normal-sync engine found no changes or completed
    /// its normal collection merge. Media remains out of scope.
    UpToDateOrMerged,
    /// A full sync would require an explicit, separate direction choice.
    /// Neither a full upload nor a full download was attempted.
    FullSyncRequiresSeparateDecision {
        upload_possible: bool,
        download_possible: bool,
    },
}

impl AnkiBackend {
    /// After the caller has explicitly approved potential remote writes,
    /// create a private offline recovery archive and run Anki's collection
    /// normal-sync protocol once. Refuses an existing backup destination.
    ///
    /// Never initiates full upload/download. Media sync requires additional
    /// work and this API does not establish full AnkiWeb compatibility.
    /// Anki note timestamps have one-second resolution; same-second
    /// independent edits have not passed conflict-convergence acceptance.
    /// The private recovery ZIP is a local fallback, not remote rollback.
    pub async fn sync_ankiweb_collection_with_recovery(
        &mut self,
        username: &str,
        password: &str,
        recovery_zip: &Path,
        approve_remote_changes: bool,
    ) -> BackendResult<CollectionSyncOutcome> {
        self.validate_sync_profile()?;
        validate_login_input(username, password)?;
        if !approve_remote_changes {
            return Err("remote collection writes require explicit approval".into());
        }
        let client = new_ankiweb_http_client()?;
        self.sync_ankiweb_collection_with_client(username, password, recovery_zip, client, None)
            .await
    }

    async fn sync_ankiweb_collection_with_client(
        &mut self,
        username: &str,
        password: &str,
        recovery_zip: &Path,
        client: reqwest::Client,
        endpoint: Option<String>,
    ) -> BackendResult<CollectionSyncOutcome> {
        // This archive must be successfully written and verified BEFORE login
        // or any call that might mutate remote or local study state.
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
        let result = self.collection.normal_sync(auth, client).await?;
        match result.required {
            SyncActionRequired::NoChanges => Ok(CollectionSyncOutcome::UpToDateOrMerged),
            SyncActionRequired::FullSyncRequired {
                upload_ok,
                download_ok,
            } => Ok(CollectionSyncOutcome::FullSyncRequiresSeparateDecision {
                upload_possible: upload_ok,
                download_possible: download_ok,
            }),
            SyncActionRequired::NormalSyncRequired => {
                Err("normal sync did not reach a completed or full-sync-required state".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    mod concurrent_edit;
    mod normal_merge;
    mod same_second;
    mod upstream_server;

    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn unapproved_collection_sync_never_produces_a_backup_or_attempts_network() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "local").unwrap();
        let destination = root.path().join("must-not-exist.zip");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let before = backend.collection_stats().unwrap();
        assert!(runtime
            .block_on(backend.sync_ankiweb_collection_with_recovery(
                "user@example.test",
                "password",
                &destination,
                false
            ))
            .is_err());
        assert!(!destination.exists());
        assert_eq!(before, backend.collection_stats().unwrap());
    }

    #[test]
    fn full_sync_requirement_preserves_private_backup_and_never_picks_direction() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "local").unwrap();
        let deck = backend.create_deck("Keep existing study data").unwrap();
        backend
            .add_basic_note(deck, "pregunta", "respuesta")
            .unwrap();
        let before = backend.collection_stats().unwrap();
        let local = backend.collection.sync_meta().unwrap();
        let mut remote = serde_json::to_value(&local).unwrap();
        // Force a remote *schema* difference, never a normal merge. A full
        // sync needs a user direction choice and must not run automatically.
        remote["mod"] = serde_json::json!(remote["mod"].as_i64().unwrap() + 100_000);
        remote["scm"] = serde_json::json!(remote["scm"].as_i64().unwrap() + 100_000);
        let payload = serde_json::to_vec(&remote).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for (index, response) in [
                br#"{"key":"safe-synthetic-key"}"#.as_slice(),
                payload.as_slice(),
            ]
            .into_iter()
            .enumerate()
            {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0_u8; 8192];
                loop {
                    let count = stream.read(&mut buf).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buf[..count]);
                    assert!(bytes.len() < 1_048_576);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]);
                        let body = &bytes[end + 4..];
                        if head
                            .to_ascii_lowercase()
                            .contains("transfer-encoding: chunked")
                        {
                            if body.windows(5).any(|w| w == b"0\r\n\r\n") {
                                break;
                            }
                        } else if let Some(len) = head.lines().find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|text| text.trim().parse::<usize>().ok())
                        }) {
                            if body.len() >= len {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                assert!(String::from_utf8_lossy(&bytes).starts_with(if index == 0 {
                    "POST /sync/hostKey "
                } else {
                    "POST /sync/meta "
                }));
                let compressed = zstd::stream::encode_all(response, 1).unwrap();
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response.len(),
                    compressed.len()
                );
                stream.write_all(headers.as_bytes()).unwrap();
                stream.write_all(&compressed).unwrap();
                stream.flush().unwrap();
            }
            // The test server only accepts login and meta; full upload or
            // download would instead fail, never be silently accepted.
        });

        let recovery = root.path().join("pre-sync-private.zip");
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let outcome = runtime
            .block_on(backend.sync_ankiweb_collection_with_client(
                "user@example.test",
                "test-password",
                &recovery,
                client,
                Some(endpoint),
            ))
            .unwrap();
        server.join().unwrap();
        assert!(matches!(
            outcome,
            CollectionSyncOutcome::FullSyncRequiresSeparateDecision { .. }
        ));
        assert!(recovery.is_file(), "private archive must precede login");
        assert!(std::fs::metadata(&recovery).unwrap().len() > 0);
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert!(backend.create_pre_sync_recovery_bundle(&recovery).is_err());
    }
}
