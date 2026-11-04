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

mod pending_changes;

/// Separate decision required because the pinned Anki normal-sync protocol
/// may complete with divergent edits that have an identical note timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionSyncConflictRisk {
    /// Refuse unverified convergence even if generic remote writes are approved.
    RefuseUnverifiedConvergence,
    /// The caller separately explained and explicitly accepted the risk that
    /// same-second note edits may remain divergent after successful sync.
    ExplicitlyAcceptPossibleSameSecondDivergence,
}

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

fn ensure_collection_sync_approval(
    approve_remote_changes: bool,
    conflict_risk: CollectionSyncConflictRisk,
) -> BackendResult<()> {
    if !approve_remote_changes {
        return Err("remote collection writes require explicit approval".into());
    }
    if conflict_risk != CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence {
        return Err(
            "collection sync refused: same-second note conflicts can diverge despite a successful sync; separate explicit acknowledgement is required".into(),
        );
    }
    Ok(())
}

impl AnkiBackend {
    /// Read-only local preflight using the SAME fail-closed predicates as the
    /// public collection sync entry point. This never logs in, creates a
    /// recovery archive, accesses AnkiWeb, or changes study records.
    ///
    /// Success only means no locally detectable blockers at this instant:
    /// older or remote-only conflicts, nonchronological IDs and media changes
    /// remain unverified. Callers must not treat this as sync approval.
    pub fn inspect_local_collection_sync_readiness(&self) -> BackendResult<()> {
        self.validate_sync_profile()?;
        pending_changes::refuse_risky_pending_note_changes(self)
    }

    /// After the caller has explicitly approved potential remote writes,
    /// create a private offline recovery archive and run Anki's collection
    /// normal-sync protocol once. Refuses an existing backup destination.
    ///
    /// Never initiates full upload/download. Media sync requires additional
    /// work and this API does not establish full AnkiWeb compatibility.
    /// Anki note timestamps have one-second resolution; same-second
    /// independent edits have not passed conflict-convergence acceptance.
    /// The private recovery ZIP is a local fallback, not remote rollback.
    /// Separate acknowledgement of same-second divergence is mandatory;
    /// declining either permission stops before backup, login or network.
    /// Fresh pending notes or cards, older possibly shared note/card edits,
    /// and pending note/card/deck deletions fail closed before login. These
    /// conservative time-based guards may reject legitimate imported changes
    /// and cannot detect remote-only edits or guarantee convergence.
    pub async fn sync_ankiweb_collection_with_recovery(
        &mut self,
        username: &str,
        password: &str,
        recovery_zip: &Path,
        approve_remote_changes: bool,
        conflict_risk: CollectionSyncConflictRisk,
    ) -> BackendResult<CollectionSyncOutcome> {
        self.validate_sync_profile()?;
        validate_login_input(username, password)?;
        ensure_collection_sync_approval(approve_remote_changes, conflict_risk)?;
        // Fail closed on risky pending shared changes and tombstones before
        // touching the network; this cannot detect remote-only conflicts.
        pending_changes::refuse_risky_pending_note_changes(self)?;
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
    mod concurrent_review;
    mod deletion_recovery;
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
    fn offline_readiness_reuses_public_guard_without_mutating_collection() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
        let before_empty = backend.collection_stats().unwrap();
        backend.inspect_local_collection_sync_readiness().unwrap();
        assert_eq!(backend.collection_stats().unwrap(), before_empty);

        let note_id = backend
            .add_basic_note(1, "offline-only readiness", "private answer")
            .unwrap();
        let before_blocked = backend.collection_stats().unwrap();
        let result = backend.inspect_local_collection_sync_readiness();
        let error = result.unwrap_err().to_string();
        assert!(error.contains("recently edited unsynced notes"), "{error}");
        assert_eq!(backend.collection_stats().unwrap(), before_blocked);
        assert_eq!(backend.note_fields(note_id).unwrap()[1], "private answer");

        // Synchronization status checks are read-only; no backup, login or
        // remote write can be triggered by inspecting local readiness.
        assert!(!root.path().join("recovery.zip").exists());
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET usn=0 WHERE id=?1", [note_id])
            .unwrap();
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE cards SET usn=0 WHERE nid=?1", [note_id])
            .unwrap();
        backend.inspect_local_collection_sync_readiness().unwrap();
    }

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
                false,
                CollectionSyncConflictRisk::RefuseUnverifiedConvergence,
            ))
            .is_err());
        assert!(!destination.exists());
        assert_eq!(before, backend.collection_stats().unwrap());
    }

    #[test]
    fn same_second_conflict_risk_requires_separate_approval_before_backup_or_network() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "local").unwrap();
        let deck = backend.create_deck("Keep local notes").unwrap();
        let note_id = backend
            .add_basic_note(deck, "private question", "private answer")
            .unwrap();
        let before = backend.collection_stats().unwrap();
        let recovery = root.path().join("must-not-exist.zip");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let denied = runtime.block_on(backend.sync_ankiweb_collection_with_recovery(
            "valid@example.test",
            "synthetic-password",
            &recovery,
            true,
            CollectionSyncConflictRisk::RefuseUnverifiedConvergence,
        ));
        assert!(denied
            .unwrap_err()
            .to_string()
            .contains("same-second note conflicts"));
        assert!(!recovery.exists());
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert_eq!(backend.note_fields(note_id).unwrap()[1], "private answer");
    }

    #[test]
    fn acknowledged_conflict_risk_does_not_replace_remote_write_permission() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "local").unwrap();
        let recovery = root.path().join("must-not-exist.zip");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let denied = runtime.block_on(backend.sync_ankiweb_collection_with_recovery(
            "valid@example.test",
            "synthetic-password",
            &recovery,
            false,
            CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
        ));
        assert!(denied
            .unwrap_err()
            .to_string()
            .contains("remote collection writes require explicit approval"));
        assert!(!recovery.exists());
        assert!(ensure_collection_sync_approval(
            true,
            CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence
        )
        .is_ok());
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
