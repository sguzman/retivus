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
    fn failed_media_request_preserves_recoverable_private_index_and_files() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "source").unwrap();
        backend.add_basic_note(1, "question", "answer").unwrap();
        let media = backend.collection.media().unwrap();
        media.add_file("original.png", b"synthetic private picture").unwrap();
        drop(media);
        let before = backend.collection_stats().unwrap();
        let archive = root.path().join("before-media-sync.zip");
        let archive_seen_by_server = archive.clone();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for request_number in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                assert!(
                    archive_seen_by_server.is_file(),
                    "recovery archive must exist before all network requests"
                );
                let mut request = Vec::new();
                let mut part = [0_u8; 8192];
                loop {
                    let count = stream.read(&mut part).unwrap();
                    assert!(count > 0, "incomplete synthetic media request");
                    request.extend_from_slice(&part[..count]);
                    assert!(request.len() < 1_048_576);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&request[..end]);
                        let body = &request[end + 4..];
                        if head
                            .to_ascii_lowercase()
                            .contains("transfer-encoding: chunked")
                        {
                            if body.windows(5).any(|w| w == b"0\r\n\r\n") {
                                break;
                            }
                        } else if let Some(size) = head.lines().find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        }) {
                            if body.len() >= size {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                let expected = if request_number == 0 {
                    "POST /sync/hostKey "
                } else {
                    "POST /msync/begin "
                };
                assert!(
                    String::from_utf8_lossy(&request).starts_with(expected),
                    "unexpected upstream endpoint"
                );
                if request_number == 0 {
                    let json = br#"{"key":"synthetic-host-key"}"#;
                    let compressed = zstd::stream::encode_all(json.as_slice(), 1).unwrap();
                    let headers = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        json.len(),
                        compressed.len()
                    );
                    stream.write_all(headers.as_bytes()).unwrap();
                    stream.write_all(&compressed).unwrap();
                } else {
                    stream
                        .write_all(
                            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                }
                stream.flush().unwrap();
            }
        });

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let result = runtime.block_on(backend.sync_ankiweb_media_with_client(
            "user@example.test",
            "synthetic-password",
            &archive,
            client,
            Some(endpoint),
        ));
        server.join().unwrap();
        assert_eq!(
            result.unwrap_err().to_string(),
            "AnkiWeb media sync failed; recovery ZIP retained"
        );
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert!(archive.is_file());

        let restored = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
            root.path(),
            "recovered",
            &archive,
        )
        .unwrap();
        assert_eq!(restored.collection_stats().unwrap(), before);
        assert_eq!(
            std::fs::read(root.path().join("recovered/collection.media/original.png")).unwrap(),
            b"synthetic private picture"
        );
        let count: i64 = rusqlite::Connection::open(root.path().join("recovered/collection.mdb"))
            .unwrap()
            .query_row("SELECT count(*) FROM media", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

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
