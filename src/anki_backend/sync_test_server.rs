//! Tests against the exact pinned upstream Anki server. Two tests may create
//! independent disposable servers concurrently; lock their brief process-wide
//! temporary credential setup so the SYNC_USER1 environment cannot race.
use anki::sync::http_server::{default_ip_header, SimpleServer, SyncServerConfig};
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::sync::Mutex;

static SYNC_TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct DisposableAnkiServer {
    pub runtime: tokio::runtime::Runtime,
    pub endpoint: String,
    pub username: &'static str,
    pub password: &'static str,
    task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl DisposableAnkiServer {
    pub fn start(root: &Path) -> Self {
        const USERNAME: &str = "retivus-test@example.invalid";
        const PASSWORD: &str = "disposable-only-password";

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let config = SyncServerConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            base_folder: root.join("upstream-server"),
            ip_header: default_ip_header(),
        };

        let (address, future) = {
            let _guard = SYNC_TEST_ENV_LOCK.lock().unwrap();
            let previous = std::env::var_os("SYNC_USER1");
            std::env::set_var("SYNC_USER1", format!("{USERNAME}:{PASSWORD}"));
            let result = runtime.block_on(SimpleServer::make_server(config));
            match previous {
                Some(value) => std::env::set_var("SYNC_USER1", value),
                None => std::env::remove_var("SYNC_USER1"),
            }
            result.expect("disposable pinned Anki server starts")
        };
        assert!(address.ip().is_loopback());
        let task = runtime.spawn(future);
        Self {
            runtime,
            endpoint: format!("http://{address}/"),
            username: USERNAME,
            password: PASSWORD,
            task,
        }
    }
}

impl Drop for DisposableAnkiServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
