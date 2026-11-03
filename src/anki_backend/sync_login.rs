//! Explicit one-shot AnkiWeb authentication followed by read-only status.
//!
//! The username and password are only passed to upstream Anki's login API;
//! Retivus does not persist credentials or the returned host key. There is
//! intentionally no automatic upload, download or conflict resolution.
use super::sync_preflight::{new_ankiweb_http_client, RemoteSyncPreflight};
use super::{AnkiBackend, BackendResult};
use anki::sync::login::sync_login;

/// Basic input validation; allow international usernames/passwords, but refuse
/// empty, oversized and control-character-bearing inputs before any network I/O.
pub(super) fn validate_login_input(username: &str, password: &str) -> BackendResult<()> {
    if username.trim().is_empty()
        || username.len() > 1024
        || username.chars().any(char::is_control)
        || password.is_empty()
        || password.len() > 8192
        || password.chars().any(char::is_control)
    {
        return Err("a valid AnkiWeb username and password must be provided".into());
    }
    Ok(())
}

pub(super) async fn obtain_temporary_host_key(
    username: &str,
    password: &str,
    endpoint: Option<String>,
    client: reqwest::Client,
) -> BackendResult<String> {
    let auth = sync_login(username.to_owned(), password.to_owned(), endpoint, client)
        .await
        // Do not format upstream errors: they can include sensitive HTTP context.
        .map_err(|_| "AnkiWeb login failed or is unavailable")?;
    if auth.hkey.is_empty()
        || auth.hkey.len() > 1024
        || auth.hkey.bytes().any(|byte| !byte.is_ascii_graphic())
    {
        return Err("AnkiWeb returned an invalid host key".into());
    }
    Ok(auth.hkey)
}

impl AnkiBackend {
    /// Opt in to an AnkiWeb login and *read-only* collection metadata preflight.
    /// Do not call from the app without an explicit user action and a recovery
    /// artifact. No credentials or server host key are saved to profile storage.
    /// This does NOT perform real synchronization or any media reconciliation.
    pub async fn authenticate_and_inspect_ankiweb_status(
        &mut self,
        username: &str,
        password: &str,
    ) -> BackendResult<RemoteSyncPreflight> {
        self.validate_sync_profile()?;
        validate_login_input(username, password)?;
        let client = new_ankiweb_http_client()?;
        let key = obtain_temporary_host_key(username, password, None, client).await?;
        self.inspect_ankiweb_sync_status(&key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anki::sync::login::SyncAuth;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    fn receive_http_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut block = [0_u8; 8192];
        loop {
            let size = stream.read(&mut block).unwrap();
            assert!(size > 0, "synthetic HTTP client sent incomplete request");
            bytes.extend_from_slice(&block[..size]);
            assert!(bytes.len() < 1_048_576, "oversized synthetic request");
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let body = &bytes[end + 4..];
                if header
                    .to_ascii_lowercase()
                    .contains("transfer-encoding: chunked")
                {
                    if body.windows(5).any(|w| w == b"0\r\n\r\n") {
                        break;
                    }
                } else if let Some(size) = header.lines().find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.trim().parse::<usize>().ok())
                }) {
                    if body.len() >= size {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn respond_zstd(stream: &mut std::net::TcpStream, json: &[u8]) {
        let payload = zstd::stream::encode_all(json, 1).unwrap();
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            json.len(),
            payload.len()
        );
        stream.write_all(headers.as_bytes()).unwrap();
        stream.write_all(&payload).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn rejects_invalid_credentials_without_accessing_any_server() {
        for (username, password) in [
            ("", "pass"),
            ("user@example.test", ""),
            ("user\nanother", "pass"),
            ("user@example.test", "pass\rword"),
        ] {
            assert!(validate_login_input(username, password).is_err());
        }
        assert!(validate_login_input("usuario@example.test", "contraseña").is_ok());
        let mut memory = AnkiBackend::in_memory().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(runtime
            .block_on(memory.authenticate_and_inspect_ankiweb_status("user", "password"))
            .is_err());

        let root = tempdir().unwrap();
        let mut managed = AnkiBackend::create_managed_profile(root.path(), "local").unwrap();
        let before = managed.collection_stats().unwrap();
        assert!(runtime
            .block_on(managed.authenticate_and_inspect_ankiweb_status("", "password"))
            .is_err());
        assert_eq!(before, managed.collection_stats().unwrap());
    }

    #[test]
    fn upstream_login_then_metadata_preflight_preserves_profile() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "study").unwrap();
        let deck = backend.create_deck("Preflight only").unwrap();
        backend.add_basic_note(deck, "Question", "Answer").unwrap();
        let before = backend.collection_stats().unwrap();
        let meta = serde_json::to_vec(&backend.collection.sync_meta().unwrap()).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for (path, payload) in [
                (
                    "/sync/hostKey",
                    br#"{"key":"synthetic-host-key"}"#.as_slice(),
                ),
                ("/sync/meta", meta.as_slice()),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let request = receive_http_request(&mut stream);
                assert!(request.starts_with(&format!("POST {path} ")));
                if path == "/sync/meta" {
                    assert!(
                        request.contains("synthetic-host-key"),
                        "metadata requests must carry the issued host key"
                    );
                }
                respond_zstd(&mut stream, payload);
            }
        });

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let key = runtime
            .block_on(obtain_temporary_host_key(
                "user@example.test",
                "test-password",
                Some(endpoint.clone()),
                client.clone(),
            ))
            .unwrap();
        assert_eq!(key, "synthetic-host-key");
        let auth = SyncAuth {
            hkey: key,
            endpoint: Some(reqwest::Url::parse(&endpoint).unwrap()),
            io_timeout_secs: Some(10),
        };
        let result = runtime
            .block_on(backend.query_remote_status_with_auth(auth, client))
            .unwrap();
        server.join().unwrap();
        assert_eq!(
            result.required,
            super::super::sync_preflight::RemoteSyncNeed::NoChanges
        );
        assert_eq!(backend.collection_stats().unwrap(), before);
    }

    #[test]
    fn authentication_rejection_does_not_change_profile() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "study").unwrap();
        let before = backend.collection_stats().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            assert!(receive_http_request(&mut stream).starts_with("POST /sync/hostKey "));
            stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime
            .block_on(obtain_temporary_host_key(
                "user@example.test",
                "incorrect",
                Some(endpoint),
                client,
            ))
            .unwrap_err();
        server.join().unwrap();
        assert_eq!(error.to_string(), "AnkiWeb login failed or is unavailable");
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert!(backend.create_deck("Unchanged after rejection").is_ok());
    }
}
