//! Optional server-side TLS. Default is plaintext (no TLS).
//!
//! Pass both `--tls-cert` and `--tls-key` (PEM) to serve gRPC over TLS. When TLS is on,
//! `--public-url` should use `https://`.

use {
    anyhow::Context,
    log::{info, warn},
    std::path::PathBuf,
    tonic::transport::{Identity, Server, ServerTlsConfig},
};

#[derive(Debug, Clone, clap::Args)]
pub struct TlsArgs {
    /// PEM certificate chain. Enables TLS together with --tls-key (default: no TLS).
    #[arg(long, env = "TLS_CERT", requires = "tls_key")]
    pub tls_cert: Option<PathBuf>,

    /// PEM private key for --tls-cert.
    #[arg(long, env = "TLS_KEY", requires = "tls_cert")]
    pub tls_key: Option<PathBuf>,
}

impl TlsArgs {
    pub fn enabled(&self) -> bool {
        self.tls_cert.is_some() && self.tls_key.is_some()
    }
}

/// `Server::builder()` with TLS applied when cert + key are configured.
pub fn server_builder(tls: &TlsArgs, public_url: &str) -> anyhow::Result<Server> {
    let builder = Server::builder();
    let (Some(cert_path), Some(key_path)) = (&tls.tls_cert, &tls.tls_key) else {
        info!("TLS: disabled (plaintext gRPC)");
        return Ok(builder);
    };

    let cert = std::fs::read(cert_path)
        .with_context(|| format!("read --tls-cert {}", cert_path.display()))?;
    let key = std::fs::read(key_path)
        .with_context(|| format!("read --tls-key {}", key_path.display()))?;
    let config = ServerTlsConfig::new().identity(Identity::from_pem(cert, key));

    if !public_url.to_ascii_lowercase().starts_with("https://") {
        warn!("TLS is enabled but --public-url={public_url} is not https://; validators will dial it without TLS");
    }
    info!("TLS: enabled (cert={})", cert_path.display());
    builder.tls_config(config).context("apply TLS config")
}
