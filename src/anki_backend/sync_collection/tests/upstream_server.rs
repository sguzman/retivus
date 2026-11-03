//! Real-server collection status and mandatory full-sync-direction gating.
//! Never uses a production AnkiWeb account and never authorizes full transfer.
use super::*;
use crate::anki_backend::sync_test_server::DisposableAnkiServer;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn real_anki_server_requires_separate_full_sync_direction_and_keeps_private_recovery() {
    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());

    let mut backend = AnkiBackend::create_managed_profile(root.path(), "client").unwrap();
    let deck = backend.create_deck("Keep my existing cards").unwrap();
    backend
        .create_bidirectional_notetype("Independent client note schema")
        .unwrap();
    backend
        .add_basic_note(deck, "local content", "not on server")
        .unwrap();
    let before = backend.collection_stats().unwrap();
    assert!(before.notes > 0);
    let recovery = root.path().join("before-upstream-collection-sync.zip");

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = server.runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(40),
            backend.sync_ankiweb_collection_with_client(
                server.username,
                server.password,
                &recovery,
                client,
                Some(server.endpoint.clone()),
            ),
        )
        .await
    });
    let outcome = result
        .expect("real upstream status request timed out")
        .expect("real upstream collection status request failed");
    assert_eq!(
        outcome,
        CollectionSyncOutcome::FullSyncRequiresSeparateDecision {
            upload_possible: true,
            download_possible: false,
        },
        "local nonempty collection and empty upstream should never sync without a direction"
    );

    assert_eq!(backend.collection_stats().unwrap(), before);
    assert!(recovery.is_file());
    assert!(backend.create_pre_sync_recovery_bundle(&recovery).is_err());

    let restored = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "restored-before-sync",
        &recovery,
    )
    .unwrap();
    assert_eq!(restored.collection_stats().unwrap(), before);
    assert!(restored
        .browse_notes("local content", 10)
        .unwrap()
        .iter()
        .any(|note| note.fields.iter().any(|field| field == "local content")));
    restored.close().unwrap();
}
