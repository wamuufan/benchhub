//! `benchhubd` — BenchHub Background Benchmark and Telemetry Daemon.

use benchhub_daemon::{
    default_socket_path, is_process_alive, parse_parent_pid_env, serve_unix_socket, DaemonRuntime,
};
use std::sync::Arc;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let socket_path = default_socket_path();
    let runtime: Arc<DaemonRuntime> = DaemonRuntime::new_default().await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    if let Some(parent_pid) = parse_parent_pid_env() {
        let rt_parent = Arc::clone(&runtime);
        let tx_parent = shutdown_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if *tx_parent.borrow() {
                    break;
                }
                if !is_process_alive(parent_pid) {
                    eprintln!(
                        "[benchhubd] Parent GUI process {} is gone; shutting down daemon",
                        parent_pid
                    );
                    rt_parent.cancel_active_for_shutdown().await;
                    let _ = tx_parent.send(true);
                    break;
                }
            }
        });
    }

    let rt_sig = Arc::clone(&runtime);
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).ok();
            let mut sigint = signal(SignalKind::interrupt()).ok();
            tokio::select! {
                _ = async {
                    if let Some(ref mut s) = sigterm {
                        s.recv().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {},
                _ = async {
                    if let Some(ref mut s) = sigint {
                        s.recv().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {},
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        rt_sig.cancel_active_for_shutdown().await;
        let _ = shutdown_tx.send(true);
    });

    eprintln!(
        "[benchhubd] Starting daemon socket at {}",
        socket_path.display()
    );
    serve_unix_socket(runtime, socket_path, shutdown_rx).await
}
