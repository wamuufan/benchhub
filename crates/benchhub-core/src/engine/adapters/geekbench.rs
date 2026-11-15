//! Geekbench 6/7 score capture adapter using a per-run local HTTPS `CONNECT` upload relay.
//!
//! Free versions of Geekbench omit scores from stdout and write no local score file; they only
//! transmit the full computed result payload over HTTPS `POST` to `browser.geekbench.com`, whose
//! public web pages sit behind a Cloudflare challenge. This adapter avoids license bypass and
//! brittle HTML scraping by running an ephemeral loopback proxy + run-scoped CA trusted only by
//! the spawned Geekbench child process, capturing scores directly from the uploaded JSON payload.

use crate::engine::adapters::{AdapterRunSession, BenchmarkAdapter, BoxFuture, FallbackResult};
use crate::engine::platform_env;
use crate::engine::BenchmarkEngine;
use crate::models::BenchmarkProfile;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use regex::Regex;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use serde_json::Value;
use std::io::{Cursor, ErrorKind, Read};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tracing::{debug, warn};

const GEEKBENCH_HOST: &str = "browser.geekbench.com";
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(250);

/// Captured scores parsed from a Geekbench upload JSON document.
#[derive(Debug, Clone, PartialEq)]
pub struct GeekbenchScoreReport {
    pub single_core: Option<f64>,
    pub multi_core: Option<f64>,
    pub compute_score: Option<f64>,
    pub compute_api: Option<String>,
}

impl GeekbenchScoreReport {
    /// Returns true if at least one numeric benchmark score was extracted.
    pub fn has_any_score(&self) -> bool {
        self.single_core.is_some() || self.multi_core.is_some() || self.compute_score.is_some()
    }

    /// Formats concise BenchHub log summary lines matching `benchmarks/geekbench.toml`'s `score_regex`.
    pub fn format_summary_lines(&self) -> String {
        let mut lines = String::from("[BenchHub] Geekbench Upload Summary:\n");
        if let Some(sc) = self.single_core {
            lines.push_str(&format!("  Single-Core Score    {}\n", format_score(sc)));
        }
        if let Some(mc) = self.multi_core {
            lines.push_str(&format!("  Multi-Core Score     {}\n", format_score(mc)));
        }
        if let Some(cs) = self.compute_score {
            let label = match self.compute_api.as_deref() {
                Some("Vulkan") => "Vulkan Score",
                Some("OpenCL") => "OpenCL Score",
                _ => "Compute Score",
            };
            lines.push_str(&format!("  {label:<20} {}\n", format_score(cs)));
        }
        lines
    }

    /// Selects the primary scalar score, matching `score_regex` against the formatted summary if
    /// supplied, otherwise preferring Multi-Core over Single-Core or Compute.
    pub fn resolve_score(&self, score_regex: &Option<Regex>) -> Option<f64> {
        if let Some(re) = score_regex {
            let summary = self.format_summary_lines();
            if let Some(matched) = BenchmarkEngine::extract_score(re.as_str(), &summary) {
                return Some(matched);
            }
        }
        self.multi_core.or(self.single_core).or(self.compute_score)
    }
}

fn format_score(val: f64) -> String {
    if (val.fract()).abs() < 1e-6 {
        format!("{:.0}", val)
    } else {
        format!("{:.2}", val)
    }
}

/// Geekbench adapter configuring per-run loopback HTTPS upload score capture.
pub struct GeekbenchAdapter;

impl BenchmarkAdapter for GeekbenchAdapter {
    fn prepare_run_session<'a>(
        &'a self,
        command: &'a mut Command,
        profile: &'a BenchmarkProfile,
        _version: &'a str,
    ) -> BoxFuture<'a, Result<Option<Box<dyn AdapterRunSession>>>> {
        Box::pin(async move {
            if profile.run_args.iter().any(|arg| arg == "--no-upload") {
                return Ok(None);
            }

            let session = GeekbenchProxySession::start(UpstreamMode::Real).await?;
            session.configure_command(command);
            Ok(Some(Box::new(session) as Box<dyn AdapterRunSession>))
        })
    }
}

#[derive(Clone)]
enum UpstreamMode {
    Real,
    #[cfg(test)]
    MockTls {
        addr: SocketAddr,
        extra_roots: Vec<CertificateDer<'static>>,
    },
}

/// Active loopback proxy session scoped cleanly to one Geekbench execution.
struct GeekbenchProxySession {
    listen_addr: SocketAddr,
    ca_bundle_path: PathBuf,
    _temp_dir: TempDir,
    captured: Arc<Mutex<Option<GeekbenchScoreReport>>>,
    shutdown_tx: watch::Sender<bool>,
    accept_task: Option<JoinHandle<()>>,
}

impl Drop for GeekbenchProxySession {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(handle) = self.accept_task.take() {
            handle.abort();
        }
    }
}

impl AdapterRunSession for GeekbenchProxySession {
    fn finish<'a>(
        mut self: Box<Self>,
        score_regex: &'a Option<Regex>,
    ) -> BoxFuture<'a, FallbackResult> {
        Box::pin(async move {
            let _ = self.shutdown_tx.send(true);
            if let Some(handle) = self.accept_task.take() {
                if tokio::time::timeout(SHUTDOWN_GRACE, handle).await.is_err() {
                    debug!("Geekbench proxy accept loop did not exit within grace period");
                }
            }

            let report = self.captured.lock().ok().and_then(|guard| guard.clone());
            match report {
                Some(rep) if rep.has_any_score() => FallbackResult {
                    output_chunks: vec![rep.format_summary_lines()],
                    score: rep.resolve_score(score_regex),
                },
                _ => FallbackResult::default(),
            }
        })
    }
}

struct EphemeralPki {
    ca_pem: String,
    server_config: Arc<ServerConfig>,
}

fn generate_ephemeral_pki() -> Result<EphemeralPki> {
    let mut ca_params = CertificateParams::new(Vec::new())
        .context("Failed to initialize ephemeral CA parameters")?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "BenchHub Ephemeral Geekbench CA");

    let ca_key = KeyPair::generate().context("Failed to generate ephemeral CA keypair")?;
    let ca_cert = ca_params
        .self_signed(&ca_key)
        .context("Failed to self-sign ephemeral Geekbench CA")?;

    let mut leaf_params = CertificateParams::new(vec![GEEKBENCH_HOST.to_string()])
        .context("Failed to initialize Geekbench leaf certificate parameters")?;
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, GEEKBENCH_HOST);
    leaf_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let leaf_key = KeyPair::generate().context("Failed to generate Geekbench leaf keypair")?;
    let leaf_cert = leaf_params
        .signed_by(&leaf_key, &ca_cert, &ca_key)
        .context("Failed to sign Geekbench leaf certificate")?;

    let cert_chain = vec![
        CertificateDer::from(leaf_cert.der().as_ref().to_vec()),
        CertificateDer::from(ca_cert.der().as_ref().to_vec()),
    ];
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut server_config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("Failed to configure TLS protocol versions for local proxy")?
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .context("Failed to configure local proxy TLS certificate")?;
    server_config.alpn_protocols = vec![b"http/1.1".to_vec()];

    Ok(EphemeralPki {
        ca_pem: ca_cert.pem(),
        server_config: Arc::new(server_config),
    })
}

fn build_upstream_client_config(mode: &UpstreamMode) -> Result<Arc<ClientConfig>> {
    let _ = mode;
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    #[cfg(test)]
    if let UpstreamMode::MockTls { extra_roots, .. } = mode {
        for cert in extra_roots {
            root_store
                .add(cert.clone())
                .context("Failed to trust mock upstream root CA")?;
        }
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("Failed to configure upstream TLS versions")?
        .with_root_certificates(root_store)
        .with_no_client_auth();
    client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(client_config))
}

fn resolve_base_ca_bundle() -> Option<PathBuf> {
    for var in ["SSL_CERT_FILE", "CURL_CA_BUNDLE"] {
        if let Some(val) = std::env::var_os(var) {
            let candidate = PathBuf::from(val);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    platform_env::find_system_ca_bundle()
}

fn write_combined_ca_bundle(dir: &Path, ephemeral_ca_pem: &str) -> Result<PathBuf> {
    let mut bundle = Vec::new();
    if let Some(base_path) = resolve_base_ca_bundle() {
        if let Ok(existing) = std::fs::read(&base_path) {
            bundle.extend_from_slice(&existing);
            if !bundle.ends_with(b"\n") {
                bundle.push(b'\n');
            }
        }
    }
    bundle.extend_from_slice(ephemeral_ca_pem.as_bytes());
    if !bundle.ends_with(b"\n") {
        bundle.push(b'\n');
    }

    let bundle_path = dir.join("ca-bundle.pem");
    std::fs::write(&bundle_path, bundle)
        .with_context(|| format!("Failed to write CA bundle at {}", bundle_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&bundle_path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(bundle_path)
}

impl GeekbenchProxySession {
    async fn start(upstream_mode: UpstreamMode) -> Result<Self> {
        let temp_dir = tempfile::Builder::new()
            .prefix("benchhub-gb-proxy-")
            .tempdir()
            .context("Failed to create temporary directory for Geekbench proxy CA")?;
        let pki = generate_ephemeral_pki()?;
        let ca_bundle_path = write_combined_ca_bundle(temp_dir.path(), &pki.ca_pem)?;
        let upstream_tls_config = build_upstream_client_config(&upstream_mode)?;

        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .context("Failed to bind loopback Geekbench upload relay listener")?;
        let listen_addr = listener.local_addr()?;

        let captured = Arc::new(Mutex::new(None));
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        let state = Arc::new(ProxyState {
            tls_acceptor: TlsAcceptor::from(pki.server_config),
            tls_connector: TlsConnector::from(upstream_tls_config),
            upstream_mode,
            captured: captured.clone(),
            shutdown_rx: shutdown_rx.clone(),
        });

        let accept_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _peer)) => {
                                let conn_state = state.clone();
                                tokio::spawn(async move {
                                    if let Err(err) = handle_client_connection(stream, conn_state).await {
                                        debug!("Geekbench proxy connection ended: {:#}", err);
                                    }
                                });
                            }
                            Err(err) => {
                                warn!("Geekbench proxy accept failed: {}", err);
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            listen_addr,
            ca_bundle_path,
            _temp_dir: temp_dir,
            captured,
            shutdown_tx,
            accept_task: Some(accept_task),
        })
    }

    fn configure_command(&self, command: &mut Command) {
        let proxy_url = format!("http://{}", self.listen_addr);
        for var in [
            "http_proxy",
            "HTTP_PROXY",
            "https_proxy",
            "HTTPS_PROXY",
            "all_proxy",
            "ALL_PROXY",
        ] {
            command.env(var, &proxy_url);
        }
        command.env_remove("no_proxy");
        command.env_remove("NO_PROXY");
        command.env("SSL_CERT_FILE", &self.ca_bundle_path);
        command.env("CURL_CA_BUNDLE", &self.ca_bundle_path);
    }
}

struct ProxyState {
    tls_acceptor: TlsAcceptor,
    tls_connector: TlsConnector,
    upstream_mode: UpstreamMode,
    captured: Arc<Mutex<Option<GeekbenchScoreReport>>>,
    shutdown_rx: watch::Receiver<bool>,
}

/// Replays unread leftover header-buffer bytes before delegating to the underlying TCP stream.
struct PrefixedStream {
    leftover: Cursor<Vec<u8>>,
    inner: TcpStream,
}

impl PrefixedStream {
    fn new(inner: TcpStream, leftover: Vec<u8>) -> Self {
        Self {
            leftover: Cursor::new(leftover),
            inner,
        }
    }
}

impl AsyncRead for PrefixedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let pos = self.leftover.position() as usize;
        let len = self.leftover.get_ref().len();
        if pos < len {
            let remaining = &self.leftover.get_ref()[pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.leftover.set_position((pos + to_copy) as u64);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn read_initial_proxy_head(stream: &mut TcpStream) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 4096];

    loop {
        if let Some(end) = find_subsequence(&buf, b"\r\n\r\n") {
            let split_at = end + 4;
            let leftover = buf.split_off(split_at);
            return Ok((buf, leftover));
        }
        if buf.len() >= MAX_HEADER_BYTES {
            anyhow::bail!("Proxy request headers exceeded {} bytes", MAX_HEADER_BYTES);
        }
        let n = tokio::time::timeout(HANDSHAKE_TIMEOUT, stream.read(&mut tmp))
            .await
            .context("Timed out waiting for proxy request header")?
            .context("Failed reading proxy request header")?;
        if n == 0 {
            anyhow::bail!("Client closed connection before completing proxy request header");
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

async fn handle_client_connection(mut client: TcpStream, state: Arc<ProxyState>) -> Result<()> {
    let _ = client.set_nodelay(true);
    let (head_bytes, leftover) = read_initial_proxy_head(&mut client).await?;

    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);
    let parsed_len = match req.parse(&head_bytes) {
        Ok(httparse::Status::Complete(len)) => len,
        _ => {
            let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            anyhow::bail!("Malformed proxy request line or headers");
        }
    };

    let method = req.method.unwrap_or("");
    let target = req.path.unwrap_or("");

    if method.eq_ignore_ascii_case("CONNECT") {
        let Some((host, port)) = parse_host_port(target, 443) else {
            let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            anyhow::bail!("Invalid CONNECT authority: {}", target);
        };
        handle_connect_tunnel(client, leftover, &host, port, state).await
    } else {
        handle_plain_http_proxy(
            client,
            &head_bytes[..parsed_len],
            leftover,
            method,
            target,
            req.headers,
            state,
        )
        .await
    }
}

async fn dial_upstream(host: &str, port: u16, mode: &UpstreamMode) -> Result<TcpStream> {
    #[cfg(test)]
    if let UpstreamMode::MockTls { addr, .. } = mode {
        if host.eq_ignore_ascii_case(GEEKBENCH_HOST) {
            let stream = TcpStream::connect(*addr).await?;
            let _ = stream.set_nodelay(true);
            return Ok(stream);
        }
    }
    let _ = mode;
    let stream = TcpStream::connect((host, port)).await?;
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

async fn handle_connect_tunnel(
    mut client: TcpStream,
    leftover: Vec<u8>,
    host: &str,
    port: u16,
    state: Arc<ProxyState>,
) -> Result<()> {
    let inspect_geekbench = host.eq_ignore_ascii_case(GEEKBENCH_HOST);

    let upstream_tcp = match dial_upstream(host, port, &state.upstream_mode).await {
        Ok(s) => s,
        Err(err) => {
            let _ = client
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n")
                .await;
            return Err(err)
                .with_context(|| format!("Failed connecting upstream {}:{}", host, port));
        }
    };

    if !inspect_geekbench {
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        client.flush().await?;
        let mut client_prefixed = PrefixedStream::new(client, leftover);
        let mut upstream = upstream_tcp;
        let mut shutdown_rx = state.shutdown_rx.clone();
        tokio::select! {
            _ = tokio::io::copy_bidirectional(&mut client_prefixed, &mut upstream) => {}
            _ = wait_for_shutdown(&mut shutdown_rx) => {}
        }
        return Ok(());
    }

    let server_name = ServerName::try_from(host.to_string())
        .context("Invalid TLS server name for Geekbench upload host")?;
    let upstream_tls = match state.tls_connector.connect(server_name, upstream_tcp).await {
        Ok(tls) => tls,
        Err(err) => {
            let _ = client
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n")
                .await;
            return Err(err).context("Upstream TLS handshake with browser.geekbench.com failed");
        }
    };

    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;
    client.flush().await?;

    let client_prefixed = PrefixedStream::new(client, leftover);
    let client_tls = state
        .tls_acceptor
        .accept(client_prefixed)
        .await
        .context("Client TLS handshake with local Geekbench relay failed")?;

    relay_with_http_tap(
        client_tls,
        upstream_tls,
        state.captured.clone(),
        state.shutdown_rx.clone(),
    )
    .await;
    Ok(())
}

async fn handle_plain_http_proxy(
    mut client: TcpStream,
    raw_head: &[u8],
    leftover: Vec<u8>,
    method: &str,
    target: &str,
    headers: &[httparse::Header<'_>],
    state: Arc<ProxyState>,
) -> Result<()> {
    let Some((host, port, origin_path)) = parse_absolute_http_target(target, headers) else {
        let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
        anyhow::bail!("Unsupported non-CONNECT proxy target: {}", target);
    };

    let Ok(upstream) = dial_upstream(&host, port, &state.upstream_mode).await else {
        let _ = client
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n")
            .await;
        anyhow::bail!("Failed connecting plain HTTP upstream {}:{}", host, port);
    };

    let rewritten_head = rewrite_proxy_request_head(raw_head, method, &origin_path);
    let inspect = host.eq_ignore_ascii_case(GEEKBENCH_HOST);

    let (mut u_read, mut u_write) = upstream.into_split();
    u_write.write_all(&rewritten_head).await?;

    let mut tap = if inspect {
        let mut t = HttpUploadTap::new(state.captured.clone());
        t.feed(&rewritten_head);
        Some(t)
    } else {
        None
    };

    if !leftover.is_empty() {
        u_write.write_all(&leftover).await?;
        if let Some(ref mut t) = tap {
            t.feed(&leftover);
        }
    }
    u_write.flush().await?;

    let (mut c_read, mut c_write) = client.into_split();
    relay_split_streams(
        &mut c_read,
        &mut c_write,
        &mut u_read,
        &mut u_write,
        tap,
        state.shutdown_rx.clone(),
    )
    .await;
    Ok(())
}

fn rewrite_proxy_request_head(raw_head: &[u8], method: &str, origin_path: &str) -> Vec<u8> {
    if let Some(first_crlf) = find_subsequence(raw_head, b"\r\n") {
        let first_line = &raw_head[..first_crlf];
        let version = first_line
            .rsplit(|&b| b == b' ')
            .next()
            .filter(|v| v.starts_with(b"HTTP/"))
            .unwrap_or(b"HTTP/1.1");
        let mut out = Vec::with_capacity(raw_head.len());
        out.extend_from_slice(method.as_bytes());
        out.push(b' ');
        out.extend_from_slice(origin_path.as_bytes());
        out.push(b' ');
        out.extend_from_slice(version);
        out.extend_from_slice(&raw_head[first_crlf..]);
        out
    } else {
        raw_head.to_vec()
    }
}

async fn wait_for_shutdown(shutdown_rx: &mut watch::Receiver<bool>) {
    if *shutdown_rx.borrow() {
        return;
    }
    while shutdown_rx.changed().await.is_ok() {
        if *shutdown_rx.borrow() {
            break;
        }
    }
}

async fn relay_with_http_tap<C, U>(
    client: C,
    upstream: U,
    captured: Arc<Mutex<Option<GeekbenchScoreReport>>>,
    shutdown_rx: watch::Receiver<bool>,
) where
    C: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    let (mut c_read, mut c_write) = tokio::io::split(client);
    let (mut u_read, mut u_write) = tokio::io::split(upstream);
    let tap = Some(HttpUploadTap::new(captured));
    relay_split_streams(
        &mut c_read,
        &mut c_write,
        &mut u_read,
        &mut u_write,
        tap,
        shutdown_rx,
    )
    .await;
}

async fn relay_split_streams<CR, CW, UR, UW>(
    c_read: &mut CR,
    c_write: &mut CW,
    u_read: &mut UR,
    u_write: &mut UW,
    mut tap: Option<HttpUploadTap>,
    mut shutdown_rx: watch::Receiver<bool>,
) where
    CR: AsyncRead + Unpin,
    CW: AsyncWrite + Unpin,
    UR: AsyncRead + Unpin,
    UW: AsyncWrite + Unpin,
{
    let c_to_u = async {
        let mut buf = [0u8; 16384];
        loop {
            match c_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if let Some(ref mut t) = tap {
                        t.feed(&buf[..n]);
                    }
                    if u_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    // Flush each chunk so HTTP/1.1 headers and Expect: 100-continue do not stall in TLS buffers.
                    if u_write.flush().await.is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(_) => break,
            }
        }
        let _ = u_write.shutdown().await;
    };

    let u_to_c = async {
        let mut buf = [0u8; 16384];
        loop {
            match u_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if c_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    if c_write.flush().await.is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(_) => break,
            }
        }
        let _ = c_write.shutdown().await;
    };

    tokio::select! {
        _ = async { tokio::join!(c_to_u, u_to_c) } => {}
        _ = wait_for_shutdown(&mut shutdown_rx) => {}
    }
}

/// Incremental HTTP/1.1 request parser capturing Geekbench upload JSON payloads without
/// delaying byte forwarding to upstream.
struct HttpUploadTap {
    buf: Vec<u8>,
    skip_remaining: usize,
    poisoned: bool,
    captured: Arc<Mutex<Option<GeekbenchScoreReport>>>,
}

impl HttpUploadTap {
    fn new(captured: Arc<Mutex<Option<GeekbenchScoreReport>>>) -> Self {
        Self {
            buf: Vec::new(),
            skip_remaining: 0,
            poisoned: false,
            captured,
        }
    }

    fn feed(&mut self, mut chunk: &[u8]) {
        if self.poisoned || chunk.is_empty() {
            return;
        }

        if self.skip_remaining > 0 {
            let take = chunk.len().min(self.skip_remaining);
            self.skip_remaining -= take;
            chunk = &chunk[take..];
            if chunk.is_empty() {
                return;
            }
        }

        if self.buf.len().saturating_add(chunk.len()) > MAX_CAPTURE_BYTES {
            warn!(
                "Geekbench upload tap buffer exceeded safety cap; disabling inspection for stream"
            );
            self.buf.clear();
            self.poisoned = true;
            return;
        }
        self.buf.extend_from_slice(chunk);

        loop {
            let Some(head_end) = find_subsequence(&self.buf, b"\r\n\r\n") else {
                if self.buf.len() > MAX_HEADER_BYTES {
                    self.buf.clear();
                    self.poisoned = true;
                }
                break;
            };
            let body_offset = head_end + 4;

            let mut headers = [httparse::EMPTY_HEADER; 64];
            let mut req = httparse::Request::new(&mut headers);
            match req.parse(&self.buf[..body_offset]) {
                Ok(httparse::Status::Complete(_)) => {}
                Ok(httparse::Status::Partial) => break,
                Err(_) => {
                    self.buf.clear();
                    self.poisoned = true;
                    break;
                }
            }

            let method = req.method.unwrap_or("").to_string();
            let target = req.path.unwrap_or("").to_string();
            let mut content_length: Option<usize> = None;
            let mut is_chunked = false;
            let mut content_type: Option<String> = None;
            let mut content_encoding: Option<String> = None;

            for hdr in req.headers.iter() {
                let name = hdr.name;
                let val = String::from_utf8_lossy(hdr.value);
                if name.eq_ignore_ascii_case("Content-Length") {
                    content_length = val.trim().parse::<usize>().ok();
                } else if name.eq_ignore_ascii_case("Transfer-Encoding")
                    && val.to_ascii_lowercase().contains("chunked")
                {
                    is_chunked = true;
                } else if name.eq_ignore_ascii_case("Content-Type") {
                    content_type = Some(val.trim().to_string());
                } else if name.eq_ignore_ascii_case("Content-Encoding") {
                    content_encoding = Some(val.trim().to_string());
                }
            }

            if is_chunked {
                match decode_chunked_body(&self.buf[body_offset..]) {
                    ChunkedState::Incomplete => break,
                    ChunkedState::Invalid => {
                        self.buf.clear();
                        self.poisoned = true;
                        break;
                    }
                    ChunkedState::Complete { consumed, decoded } => {
                        self.process_request(
                            &method,
                            &target,
                            content_type.as_deref(),
                            content_encoding.as_deref(),
                            &decoded,
                        );
                        self.buf.drain(..body_offset + consumed);
                    }
                }
            } else if let Some(len) = content_length {
                if len > MAX_CAPTURE_BYTES {
                    let available_body = self.buf.len().saturating_sub(body_offset);
                    if available_body >= len {
                        self.buf.drain(..body_offset + len);
                        continue;
                    } else {
                        self.skip_remaining = len - available_body;
                        self.buf.clear();
                        break;
                    }
                }

                let total_len = match body_offset.checked_add(len) {
                    Some(v) => v,
                    None => {
                        self.buf.clear();
                        self.poisoned = true;
                        break;
                    }
                };
                if self.buf.len() < total_len {
                    break;
                }
                let body = self.buf[body_offset..total_len].to_vec();
                self.process_request(
                    &method,
                    &target,
                    content_type.as_deref(),
                    content_encoding.as_deref(),
                    &body,
                );
                self.buf.drain(..total_len);
            } else {
                // Requests with neither Content-Length nor chunked framing carry zero body bytes.
                self.buf.drain(..body_offset);
            }
        }
    }

    fn process_request(
        &self,
        method: &str,
        target: &str,
        content_type: Option<&str>,
        content_encoding: Option<&str>,
        body: &[u8],
    ) {
        if !method.eq_ignore_ascii_case("POST") {
            return;
        }
        if let Some(report) =
            extract_score_from_http_post(target, content_type, content_encoding, body)
        {
            if report.has_any_score() {
                if let Ok(mut guard) = self.captured.lock() {
                    *guard = Some(report);
                }
            }
        }
    }
}

enum ChunkedState {
    Incomplete,
    Invalid,
    Complete { consumed: usize, decoded: Vec<u8> },
}

fn decode_chunked_body(data: &[u8]) -> ChunkedState {
    let mut pos = 0usize;
    let mut decoded = Vec::new();

    loop {
        let Some(rel_crlf) = find_subsequence(&data[pos..], b"\r\n") else {
            return ChunkedState::Incomplete;
        };
        let size_line = &data[pos..pos + rel_crlf];
        let size_str = std::str::from_utf8(size_line)
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let Ok(chunk_len) = usize::from_str_radix(size_str, 16) else {
            return ChunkedState::Invalid;
        };
        pos += rel_crlf + 2;

        if chunk_len == 0 {
            if data[pos..].starts_with(b"\r\n") {
                return ChunkedState::Complete {
                    consumed: pos + 2,
                    decoded,
                };
            }
            if let Some(trailer_end) = find_subsequence(&data[pos..], b"\r\n\r\n") {
                return ChunkedState::Complete {
                    consumed: pos + trailer_end + 4,
                    decoded,
                };
            }
            return ChunkedState::Incomplete;
        }

        if decoded.len().saturating_add(chunk_len) > MAX_CAPTURE_BYTES {
            return ChunkedState::Invalid;
        }
        let Some(chunk_end) = pos.checked_add(chunk_len).and_then(|v| v.checked_add(2)) else {
            return ChunkedState::Invalid;
        };
        if data.len() < chunk_end {
            return ChunkedState::Incomplete;
        }
        if &data[pos + chunk_len..chunk_end] != b"\r\n" {
            return ChunkedState::Invalid;
        }
        decoded.extend_from_slice(&data[pos..pos + chunk_len]);
        pos = chunk_end;
    }
}

/// Extracts a [`GeekbenchScoreReport`] from a captured Geekbench HTTP `POST` body.
pub fn extract_score_from_http_post(
    target: &str,
    content_type: Option<&str>,
    content_encoding: Option<&str>,
    raw_body: &[u8],
) -> Option<GeekbenchScoreReport> {
    let body_storage: Vec<u8>;
    let body: &[u8] = if content_encoding
        .map(|enc| enc.to_ascii_lowercase().contains("gzip"))
        .unwrap_or(false)
    {
        body_storage = gunzip_bounded(raw_body)?;
        &body_storage
    } else {
        raw_body
    };

    if let Some(ct) = content_type {
        if ct.to_ascii_lowercase().contains("multipart/form-data") {
            if let Some(boundary) = extract_multipart_boundary(ct) {
                if let Some(doc_bytes) = extract_multipart_field(body, &boundary, "document") {
                    let doc_json = parse_json_maybe_gzip(&doc_bytes)?;
                    return parse_geekbench_document_json(target, &doc_json);
                }
            }
        }
    }

    let trimmed_start = body.iter().position(|b| !b.is_ascii_whitespace())?;
    if body[trimmed_start] == b'{' {
        let json: Value = serde_json::from_slice(&body[trimmed_start..]).ok()?;
        return parse_geekbench_document_json(target, &json);
    }

    None
}

fn gunzip_bounded(data: &[u8]) -> Option<Vec<u8>> {
    let decoder = GzDecoder::new(data);
    let mut limited = decoder.take((MAX_CAPTURE_BYTES + 1) as u64);
    let mut out = Vec::new();
    limited.read_to_end(&mut out).ok()?;
    if out.len() > MAX_CAPTURE_BYTES {
        return None;
    }
    Some(out)
}

fn parse_json_maybe_gzip(data: &[u8]) -> Option<Value> {
    if data.starts_with(&[0x1f, 0x8b]) {
        let decoded = gunzip_bounded(data)?;
        return serde_json::from_slice(&decoded).ok();
    }
    serde_json::from_slice(data).ok()
}

fn extract_multipart_boundary(content_type: &str) -> Option<String> {
    for part in content_type.split(';') {
        let trimmed = part.trim();
        if trimmed.len() >= 9 && trimmed[..9].eq_ignore_ascii_case("boundary=") {
            let value = trimmed[9..].trim();
            let unquoted = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            if !unquoted.is_empty() {
                return Some(unquoted.to_string());
            }
        }
    }
    None
}

fn extract_multipart_field(body: &[u8], boundary: &str, field_name: &str) -> Option<Vec<u8>> {
    let marker = format!("--{}", boundary);
    let marker_bytes = marker.as_bytes();

    let mut positions = Vec::new();
    let mut search_from = 0usize;
    while search_from < body.len() {
        let Some(rel) = find_subsequence(&body[search_from..], marker_bytes) else {
            break;
        };
        let abs = search_from + rel;
        let at_line_start =
            abs == 0 || body[..abs].ends_with(b"\r\n") || body[..abs].ends_with(b"\n");
        let after = abs + marker_bytes.len();
        let valid_suffix = after >= body.len()
            || body[after..].starts_with(b"\r\n")
            || body[after..].starts_with(b"\n")
            || body[after..].starts_with(b"--");
        if at_line_start && valid_suffix {
            positions.push(abs);
        }
        search_from = after;
    }

    for pair in positions.windows(2) {
        let part_marker = pair[0];
        let next_marker = pair[1];
        let mut cursor = part_marker + marker_bytes.len();
        if body[cursor..].starts_with(b"\r\n") {
            cursor += 2;
        } else if body[cursor..].starts_with(b"\n") {
            cursor += 1;
        } else {
            continue;
        }

        let part_slice = &body[cursor..next_marker];
        let (header_bytes, value_slice) =
            if let Some(h_end) = find_subsequence(part_slice, b"\r\n\r\n") {
                (&part_slice[..h_end], &part_slice[h_end + 4..])
            } else if let Some(h_end) = find_subsequence(part_slice, b"\n\n") {
                (&part_slice[..h_end], &part_slice[h_end + 2..])
            } else {
                continue;
            };

        let headers_str = String::from_utf8_lossy(header_bytes);
        if !disposition_matches_name(&headers_str, field_name) {
            continue;
        }

        let trimmed_value = value_slice
            .strip_suffix(b"\r\n")
            .or_else(|| value_slice.strip_suffix(b"\n"))
            .unwrap_or(value_slice);

        if part_headers_specify_gzip(&headers_str) {
            return gunzip_bounded(trimmed_value);
        }
        return Some(trimmed_value.to_vec());
    }

    None
}

fn disposition_matches_name(headers_str: &str, field_name: &str) -> bool {
    let needle_quoted = format!("name=\"{}\"", field_name);
    let needle_unquoted = format!("name={}", field_name);
    for line in headers_str.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-disposition:") {
            if lower.contains(&needle_quoted.to_ascii_lowercase()) {
                return true;
            }
            if let Some(pos) = lower.find(&needle_unquoted.to_ascii_lowercase()) {
                let end = pos + needle_unquoted.len();
                let next_char = lower.as_bytes().get(end).copied();
                if matches!(
                    next_char,
                    None | Some(b';') | Some(b' ') | Some(b'\t') | Some(b'\r')
                ) {
                    return true;
                }
            }
        }
    }
    false
}

fn part_headers_specify_gzip(headers_str: &str) -> bool {
    headers_str.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("content-encoding:") && lower.contains("gzip")
    })
}

/// Parses Geekbench single-core, multi-core, or compute scores from an upload `document` JSON object.
pub fn parse_geekbench_document_json(target: &str, doc: &Value) -> Option<GeekbenchScoreReport> {
    let top_score = json_numeric_score(doc.get("score"));
    let top_multicore = json_numeric_score(doc.get("multicore_score"));
    let section_single =
        find_section_score(doc, &["single-core", "single core"]).filter(|&s| s > 0.0);
    let section_multi = find_section_score(doc, &["multi-core", "multi core"]).filter(|&s| s > 0.0);

    let target_lower = target.to_ascii_lowercase();
    let is_gpu_target = target_lower.contains("/gpu/") || target_lower.contains("/compute/");
    let is_cpu_target = target_lower.contains("/cpu/");
    let positive_top_score = top_score.filter(|&s| s > 0.0);
    let positive_top_multicore = top_multicore.filter(|&s| s > 0.0);

    // Geekbench GPU uploads include `"multicore_score": 0`; require a positive value or `/cpu/` path.
    let is_cpu = if is_gpu_target {
        false
    } else {
        is_cpu_target
            || positive_top_multicore.is_some()
            || section_single.is_some()
            || section_multi.is_some()
    };

    if is_cpu {
        let single_core = positive_top_score.or(section_single).or(top_score);
        let multi_core = positive_top_multicore.or(section_multi);
        if single_core.is_none() && multi_core.is_none() {
            return None;
        }
        return Some(GeekbenchScoreReport {
            single_core,
            multi_core,
            compute_score: None,
            compute_api: None,
        });
    }

    let compute_score = positive_top_score
        .or_else(|| first_section_score(doc))
        .or(top_score);
    compute_score?;
    let compute_api = detect_compute_api(target, doc);

    Some(GeekbenchScoreReport {
        single_core: None,
        multi_core: None,
        compute_score,
        compute_api,
    })
}

fn json_numeric_score(val: Option<&Value>) -> Option<f64> {
    let v = val?;
    match v {
        Value::Number(n) => n.as_f64().filter(|x| x.is_finite() && *x >= 0.0),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|x| x.is_finite() && *x >= 0.0),
        _ => None,
    }
}

fn find_section_score(doc: &Value, name_needles: &[&str]) -> Option<f64> {
    let sections = doc.get("sections")?.as_array()?;
    for sec in sections {
        let sec_name = sec
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name_needles.iter().any(|needle| sec_name.contains(needle)) {
            if let Some(s) = json_numeric_score(sec.get("score")) {
                return Some(s);
            }
        }
    }
    None
}

fn first_section_score(doc: &Value) -> Option<f64> {
    let sections = doc.get("sections")?.as_array()?;
    for sec in sections {
        if let Some(s) = json_numeric_score(sec.get("score")).filter(|&v| v > 0.0) {
            return Some(s);
        }
    }
    None
}

fn normalize_compute_api(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("vulkan") {
        Some("Vulkan".to_string())
    } else if lower.contains("opencl") {
        Some("OpenCL".to_string())
    } else {
        None
    }
}

fn detect_compute_api(target: &str, doc: &Value) -> Option<String> {
    let from_obj_keys = |obj: &Value, keys: &[&str]| -> Option<String> {
        keys.iter()
            .filter_map(|key| obj.get(key).and_then(|v| v.as_str()))
            .find_map(normalize_compute_api)
    };

    if let Some(api) = normalize_compute_api(target)
        .or_else(|| from_obj_keys(doc, &["gpu_api", "compute_api", "api", "backend"]))
        .or_else(|| {
            doc.get("options")
                .and_then(|opts| from_obj_keys(opts, &["gpu_api", "compute_api", "api", "backend"]))
        })
    {
        return Some(api);
    }

    doc.get("sections")
        .and_then(|v| v.as_array())
        .and_then(|sections| {
            sections
                .iter()
                .find_map(|sec| from_obj_keys(sec, &["name", "api", "compute_api"]))
        })
}

fn parse_host_port(authority: &str, default_port: u16) -> Option<(String, u16)> {
    let trimmed = authority.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        let close = rest.find(']')?;
        let host = &rest[..close];
        let after = &rest[close + 1..];
        let port = if let Some(p) = after.strip_prefix(':') {
            p.parse::<u16>().ok()?
        } else if after.is_empty() {
            default_port
        } else {
            return None;
        };
        return Some((host.to_string(), port));
    }

    if let Some((host, port_str)) = trimmed.rsplit_once(':') {
        if !host.is_empty() && !host.contains(':') {
            let port = port_str.parse::<u16>().ok()?;
            return Some((host.to_string(), port));
        }
    }
    Some((trimmed.to_string(), default_port))
}

fn parse_absolute_http_target(
    target: &str,
    headers: &[httparse::Header<'_>],
) -> Option<(String, u16, String)> {
    if let Some(rest) = target.strip_prefix("http://") {
        let (auth, path) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, "/"),
        };
        let (host, port) = parse_host_port(auth, 80)?;
        return Some((host, port, path.to_string()));
    }
    if target.starts_with('/') {
        for hdr in headers {
            if hdr.name.eq_ignore_ascii_case("Host") {
                let auth = String::from_utf8_lossy(hdr.value);
                let (host, port) = parse_host_port(&auth, 80)?;
                return Some((host, port, target.to_string()));
            }
        }
    }
    None
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use rustls_pemfile_helpers::load_first_cert_der;
    use std::io::Write;
    use tokio::io::{AsyncBufReadExt, BufReader};

    mod rustls_pemfile_helpers {
        use rustls::pki_types::pem::PemObject;
        use rustls::pki_types::CertificateDer;

        pub fn load_first_cert_der(pem: &str) -> Vec<CertificateDer<'static>> {
            CertificateDer::pem_slice_iter(pem.as_bytes())
                .filter_map(|item| item.ok())
                .collect()
        }
    }

    #[test]
    fn parses_geekbench_multipart_cpu_upload_and_resolves_scores() {
        let boundary = "------------------------a7d03468d5cf5d41";
        let doc = serde_json::json!({
            "version": "7.0.0",
            "score": 2028,
            "multicore_score": 12633,
            "sections": [
                {"id": 1, "name": "Single-Core", "score": 2028},
                {"id": 2, "name": "Multi-Core", "score": 12633}
            ]
        })
        .to_string();
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"document\"\r\n\r\n{doc}\r\n--{b}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n53206e0175246c2f0b09e89079d30c11\r\n--{b}--\r\n",
            b = boundary,
            doc = doc
        );
        let content_type = format!("multipart/form-data; boundary={}", boundary);
        let report = extract_score_from_http_post(
            "/v7/cpu/upload.json",
            Some(&content_type),
            None,
            body.as_bytes(),
        )
        .expect("report should parse");

        assert_eq!(report.single_core, Some(2028.0));
        assert_eq!(report.multi_core, Some(12633.0));
        assert_eq!(report.compute_score, None);

        let default_regex =
            Regex::new(r"(?:Single-Core|Multi-Core|Compute|OpenCL|Vulkan) Score\s+(\d+)").ok();
        assert_eq!(report.resolve_score(&default_regex), Some(12633.0));

        let single_only_regex = Regex::new(r"Single-Core Score\s+(\d+)").ok();
        assert_eq!(report.resolve_score(&single_only_regex), Some(2028.0));
    }

    #[test]
    fn parses_geekbench_vulkan_compute_and_gzip_chunked_payloads() {
        let doc = serde_json::json!({
            "version": "7.0.0",
            "score": 58420,
            "sections": [
                {"name": "Vulkan", "score": 58420}
            ]
        })
        .to_string();

        let boundary = "gbBound99";
        let raw_body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"document\"\r\n\r\n{doc}\r\n--{boundary}--\r\n"
        );
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(raw_body.as_bytes()).unwrap();
        let compressed = gz.finish().unwrap();

        let mid = compressed.len() / 2;
        let chunked = format!("{:x}\r\n", mid).into_bytes();
        let mut full_stream = Vec::new();
        full_stream.extend_from_slice(
            format!(
                "POST /v7/compute/upload.json HTTP/1.1\r\nHost: browser.geekbench.com\r\nTransfer-Encoding: chunked\r\nContent-Encoding: gzip\r\nContent-Type: multipart/form-data; boundary=\"{boundary}\"\r\n\r\n"
            )
            .as_bytes(),
        );
        full_stream.extend_from_slice(&chunked);
        full_stream.extend_from_slice(&compressed[..mid]);
        full_stream.extend_from_slice(b"\r\n");
        full_stream.extend_from_slice(format!("{:x}\r\n", compressed.len() - mid).as_bytes());
        full_stream.extend_from_slice(&compressed[mid..]);
        full_stream.extend_from_slice(b"\r\n0\r\n\r\n");

        let captured = Arc::new(Mutex::new(None));
        let mut tap = HttpUploadTap::new(captured.clone());
        // Feed in tiny uneven slices to verify incremental reassembly across chunk boundaries.
        for slice in full_stream.chunks(17) {
            tap.feed(slice);
        }

        let report = captured
            .lock()
            .unwrap()
            .clone()
            .expect("chunked upload captured");
        assert_eq!(report.compute_score, Some(58420.0));
        assert_eq!(report.compute_api.as_deref(), Some("Vulkan"));
        let regex =
            Regex::new(r"(?:Single-Core|Multi-Core|Compute|OpenCL|Vulkan) Score\s+(\d+)").ok();
        assert_eq!(report.resolve_score(&regex), Some(58420.0));
        assert!(report
            .format_summary_lines()
            .contains("Vulkan Score         58420"));
    }

    #[test]
    fn parse_host_port_handles_ipv4_hostname_and_ipv6() {
        assert_eq!(
            parse_host_port("browser.geekbench.com:443", 443),
            Some(("browser.geekbench.com".to_string(), 443))
        );
        assert_eq!(
            parse_host_port("browser.geekbench.com", 443),
            Some(("browser.geekbench.com".to_string(), 443))
        );
        assert_eq!(
            parse_host_port("[::1]:8443", 443),
            Some(("::1".to_string(), 8443))
        );
    }

    #[tokio::test]
    async fn end_to_end_connect_tls_relay_captures_score_and_relays_upstream_response() {
        tokio::time::timeout(Duration::from_secs(10), async {
            // Build a self-signed mock upstream TLS server pretending to be browser.geekbench.com.
            let mut mock_ca_params = CertificateParams::new(Vec::new()).unwrap();
            mock_ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            mock_ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
            let mock_ca_key = KeyPair::generate().unwrap();
            let mock_ca_cert = mock_ca_params.self_signed(&mock_ca_key).unwrap();

            let mut mock_leaf_params = CertificateParams::new(vec![GEEKBENCH_HOST.to_string()]).unwrap();
            mock_leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
            let mock_leaf_key = KeyPair::generate().unwrap();
            let mock_leaf_cert = mock_leaf_params
                .signed_by(&mock_leaf_key, &mock_ca_cert, &mock_ca_key)
                .unwrap();

            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let mut upstream_server_cfg = ServerConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(mock_leaf_cert.der().as_ref().to_vec())],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(mock_leaf_key.serialize_der())),
                )
                .unwrap();
            upstream_server_cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
            let upstream_acceptor = TlsAcceptor::from(Arc::new(upstream_server_cfg));

            let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let upstream_addr = upstream_listener.local_addr().unwrap();

            let server_task = tokio::spawn(async move {
                let (tcp, _) = upstream_listener.accept().await.unwrap();
                let mut tls = upstream_acceptor.accept(tcp).await.unwrap();
                let mut req_buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = tls.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    req_buf.extend_from_slice(&chunk[..n]);
                    if find_subsequence(&req_buf, b"--testbound--\r\n").is_some() {
                        break;
                    }
                }
                let body = r#"{"id":875370,"key":621794}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                tls.write_all(resp.as_bytes()).await.unwrap();
                tls.flush().await.unwrap();
                tls.shutdown().await.unwrap();
            });

            let mock_ca_der = CertificateDer::from(mock_ca_cert.der().as_ref().to_vec());
            let session = GeekbenchProxySession::start(UpstreamMode::MockTls {
                addr: upstream_addr,
                extra_roots: vec![mock_ca_der],
            })
            .await
            .unwrap();

            // Client trusts ONLY the CA bundle generated by GeekbenchProxySession for child processes.
            let ca_pem = std::fs::read_to_string(&session.ca_bundle_path).unwrap();
            let mut client_roots = RootCertStore::empty();
            for cert in load_first_cert_der(&ca_pem) {
                let _ = client_roots.add(cert);
            }
            let client_cfg = ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(client_roots)
                .with_no_client_auth();
            let client_connector = TlsConnector::from(Arc::new(client_cfg));

            let mut proxy_tcp = TcpStream::connect(session.listen_addr).await.unwrap();
            proxy_tcp
                .write_all(b"CONNECT browser.geekbench.com:443 HTTP/1.1\r\nHost: browser.geekbench.com:443\r\n\r\n")
                .await
                .unwrap();
            proxy_tcp.flush().await.unwrap();

            let mut reader = BufReader::new(proxy_tcp);
            let mut connect_status = String::new();
            reader.read_line(&mut connect_status).await.unwrap();
            assert!(connect_status.starts_with("HTTP/1.1 200"));
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let proxy_tcp = reader.into_inner();

            let mut client_tls = client_connector
                .connect(ServerName::try_from(GEEKBENCH_HOST.to_string()).unwrap(), proxy_tcp)
                .await
                .unwrap();

            let boundary = "testbound";
            let json_doc = r#"{"score":2150,"multicore_score":13400,"sections":[{"name":"Single-Core","score":2150},{"name":"Multi-Core","score":13400}]}"#;
            let multipart = format!(
                "--{b}\r\nContent-Disposition: form-data; name=\"document\"\r\n\r\n{json_doc}\r\n--{b}--\r\n",
                b = boundary
            );
            let req = format!(
                "POST /v7/cpu/upload.json HTTP/1.1\r\nHost: browser.geekbench.com\r\nContent-Type: multipart/form-data; boundary={b}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                multipart.len(),
                multipart,
                b = boundary
            );
            client_tls.write_all(req.as_bytes()).await.unwrap();
            client_tls.flush().await.unwrap();

            let mut resp_bytes = Vec::new();
            let _ = client_tls.read_to_end(&mut resp_bytes).await;
            let resp_str = String::from_utf8_lossy(&resp_bytes);
            assert!(
                resp_str.contains(r#"{"id":875370,"key":621794}"#),
                "Unexpected relayed response: {}",
                resp_str
            );

            let fallback = Box::new(session)
                .finish(&Regex::new(r"(?:Single-Core|Multi-Core|Compute|OpenCL|Vulkan) Score\s+(\d+)").ok())
                .await;
            assert_eq!(fallback.score, Some(13400.0));
            let joined = fallback.output_chunks.join("");
            assert!(joined.contains("Single-Core Score    2150"));
            assert!(joined.contains("Multi-Core Score     13400"));

            server_task.await.unwrap();
        })
        .await
        .expect("TLS relay integration test timed out");
    }

    #[tokio::test]
    async fn non_geekbench_connect_tunnel_relays_blindly_without_tls_interception() {
        tokio::time::timeout(Duration::from_secs(10), async {
            let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let upstream_addr = upstream_listener.local_addr().unwrap();

            let echo_task = tokio::spawn(async move {
                let (mut stream, _) = upstream_listener.accept().await.unwrap();
                let mut buf = [0u8; 64];
                let n = stream.read(&mut buf).await.unwrap();
                stream.write_all(&buf[..n]).await.unwrap();
                stream.shutdown().await.unwrap();
            });

            let session = GeekbenchProxySession::start(UpstreamMode::Real)
                .await
                .unwrap();

            let mut proxy_tcp = TcpStream::connect(session.listen_addr).await.unwrap();
            let connect_req = format!("CONNECT {0} HTTP/1.1\r\nHost: {0}\r\n\r\n", upstream_addr);
            proxy_tcp.write_all(connect_req.as_bytes()).await.unwrap();
            proxy_tcp.flush().await.unwrap();

            let mut reader = BufReader::new(proxy_tcp);
            let mut status = String::new();
            reader.read_line(&mut status).await.unwrap();
            assert!(status.starts_with("HTTP/1.1 200"));
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let mut tunnel = reader.into_inner();
            tunnel.write_all(b"RAW-NON-TLS-PING").await.unwrap();
            tunnel.flush().await.unwrap();

            let mut reply = vec![0u8; 16];
            tunnel.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"RAW-NON-TLS-PING");

            let fallback = Box::new(session).finish(&None).await;
            assert_eq!(fallback.score, None);
            assert!(fallback.output_chunks.is_empty());

            echo_task.await.unwrap();
        })
        .await
        .expect("blind tunnel test timed out");
    }

    #[tokio::test]
    async fn skips_proxy_setup_when_no_upload_flag_is_present() {
        let adapter = GeekbenchAdapter;
        let profile = BenchmarkProfile {
            id: "geekbench".to_string(),
            name: "Geekbench".to_string(),
            run_cmd: "./geekbench7".to_string(),
            run_args: vec!["--no-upload".to_string()],
            ..Default::default()
        };
        let mut cmd = Command::new("./geekbench7");
        let session = adapter
            .prepare_run_session(&mut cmd, &profile, "7.0.0")
            .await
            .unwrap();
        assert!(session.is_none());
    }

    #[tokio::test]
    #[ignore = "requires local Geekbench 7 runner and internet"]
    async fn live_geekbench7_vulkan_captures_uploaded_score_via_engine() {
        let data_root = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .expect("HOME or XDG_DATA_HOME must be set");
        let runners_root = data_root.join("benchhub").join("runners");
        assert!(
            runners_root.join("geekbench/7.0.0").exists(),
            "missing local runner at {}",
            runners_root.display()
        );

        let temp = tempfile::tempdir().unwrap();
        let engine =
            BenchmarkEngine::new(temp.path().to_path_buf()).with_runners_dir(Some(runners_root));

        let profile = BenchmarkProfile {
            id: "geekbench".to_string(),
            name: "Geekbench".to_string(),
            category: "CPU / Compute".to_string(),
            run_cmd: "./geekbench7".to_string(),
            score_regex: Some(
                r"(?:Single-Core|Multi-Core|Compute|OpenCL|Vulkan) Score\s+(\d+)".to_string(),
            ),
            ..Default::default()
        };
        let custom_args = vec![
            "--gpu".to_string(),
            "Vulkan".to_string(),
            "--upload".to_string(),
        ];
        let log_path = temp.path().join("geekbench7-vulkan.log");
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let collected = Arc::new(Mutex::new(String::new()));
        let collected_cb = Arc::clone(&collected);

        let output = engine
            .run_version_custom(
                &profile,
                "7.0.0",
                Some(&custom_args),
                Some(300),
                cancel_rx,
                Some(log_path),
                crate::models::GpuMode::Auto,
                move |chunk| {
                    if let Ok(mut buf) = collected_cb.lock() {
                        buf.push_str(&chunk);
                    }
                },
            )
            .await
            .expect("live geekbench7 execution should complete");

        let captured_log = collected.lock().unwrap().clone();
        println!("Live Geekbench 7 result: {:?}", output);
        println!("Live Geekbench 7 log:\n{}", captured_log);

        assert_eq!(output.status, "Success");
        let captured_score = output.score.expect("expected captured Geekbench score");
        assert!(
            captured_score > 0.0,
            "expected positive GPU score, got {}",
            captured_score
        );
        assert!(
            !captured_log.contains("Multi-Core Score"),
            "GPU compute run should not emit Multi-Core Score"
        );
        assert!(
            captured_log.contains("Vulkan Score") || captured_log.contains("Compute Score"),
            "expected summary line in captured output"
        );
    }

    #[test]
    fn gpu_upload_with_zero_multicore_score_is_not_misclassified_as_cpu() {
        let doc = serde_json::json!({
            "version": "7.0.0",
            "score": 24566,
            "multicore_score": 0,
            "sections": [{"name": "Compute", "score": 24566}]
        });
        let report = parse_geekbench_document_json("/v7/gpu/upload.json", &doc)
            .expect("gpu report should parse");

        assert_eq!(report.single_core, None);
        assert_eq!(report.multi_core, None);
        assert_eq!(report.compute_score, Some(24566.0));
        let default_regex =
            Regex::new(r"(?:Single-Core|Multi-Core|Compute|OpenCL|Vulkan) Score\s+(\d+)").ok();
        assert_eq!(report.resolve_score(&default_regex), Some(24566.0));
        assert!(report
            .format_summary_lines()
            .contains("Compute Score        24566"));
    }

    #[test]
    fn cpu_single_core_only_with_zero_multicore_prefers_single_core_score() {
        let doc = serde_json::json!({
            "version": "7.0.0",
            "score": 2850,
            "multicore_score": 0
        });
        let report = parse_geekbench_document_json("/v7/cpu/upload.json", &doc)
            .expect("cpu report should parse");

        assert_eq!(report.single_core, Some(2850.0));
        assert_eq!(report.multi_core, None);
        let default_regex =
            Regex::new(r"(?:Single-Core|Multi-Core|Compute|OpenCL|Vulkan) Score\s+(\d+)").ok();
        assert_eq!(report.resolve_score(&default_regex), Some(2850.0));
    }
}
