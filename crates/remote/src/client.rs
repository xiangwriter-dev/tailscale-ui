use crate::identity::{certificate_der, tailscale_address, Endpoint, PairingOffer};
use reqwest::{Client, Method};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{net::IpAddr, sync::Arc, time::Duration};
use tailtask_core::remote::{hex_digest, PairRequest, PairResponse, RemoteConnection};

#[derive(Debug)]
struct PinnedVerifier {
    certificate: Vec<u8>,
    inner: Arc<dyn ServerCertVerifier>,
}
impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.as_ref() != self.certificate.as_slice() {
            return Err(rustls::Error::General("PINNED_CERTIFICATE_CHANGED".into()));
        }
        self.inner
            .verify_server_cert(end_entity, intermediates, name, ocsp, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[derive(Clone)]
pub struct RemoteClient {
    client: Client,
    base: String,
    token: Option<String>,
}
impl RemoteClient {
    pub async fn download(
        &self,
        artifact: &tailtask_core::remote::Artifact,
        destination: &std::path::Path,
    ) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        uuid::Uuid::parse_str(&artifact.id).map_err(|_| "INVALID_ARTIFACT_ID")?;
        uuid::Uuid::parse_str(&artifact.task_id).map_err(|_| "INVALID_TASK_ID")?;
        if artifact.size > 500 * 1024 * 1024 {
            return Err("ARTIFACT_SIZE_LIMIT".into());
        }
        if destination.exists() {
            return Err("DOWNLOAD_DESTINATION_EXISTS".into());
        }
        let parent = destination
            .parent()
            .ok_or("DOWNLOAD_PARENT_REQUIRED")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let filename = destination
            .file_name()
            .ok_or("DOWNLOAD_FILENAME_REQUIRED")?;
        let target = parent.join(filename);
        let partial = parent.join(format!(
            ".{}.{}.partial",
            filename.to_string_lossy(),
            artifact.id
        ));
        if tokio::fs::symlink_metadata(&partial)
            .await
            .is_ok_and(|m| m.file_type().is_symlink() || !m.is_file())
        {
            return Err("DOWNLOAD_PARTIAL_NOT_REGULAR".into());
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
            options.custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
        let mut file = options.open(&partial).await.map_err(|e| e.to_string())?;
        let retained = file.metadata().await.map_err(|e| e.to_string())?.len();
        if retained > artifact.size {
            return Err("DOWNLOAD_PARTIAL_SIZE_MISMATCH".into());
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer).await.map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        if retained < artifact.size {
            let mut request = self.client.get(format!(
                "{}/v1/tasks/{}/artifacts/{}",
                self.base, artifact.task_id, artifact.id
            ));
            if let Some(token) = &self.token {
                request = request.bearer_auth(token);
            }
            if retained > 0 {
                request = request.header("range", format!("bytes={retained}-"));
            }
            let mut response = request
                .send()
                .await
                .map_err(|_| "DOWNLOAD_INTERRUPTED: 已保留 .partial，可重试")?;
            if (retained > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT)
                || (retained == 0 && response.status() != reqwest::StatusCode::OK)
            {
                return Err("DOWNLOAD_RESPONSE_MISMATCH".into());
            }
            if retained > 0 {
                let expected =
                    format!("bytes {}-{}/{}", retained, artifact.size - 1, artifact.size);
                if response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    != Some(expected.as_str())
                {
                    return Err("DOWNLOAD_RANGE_MISMATCH".into());
                }
            }
            let mut size = retained;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "DOWNLOAD_INTERRUPTED: 已保留 .partial，可重试")?
            {
                size += chunk.len() as u64;
                if size > artifact.size {
                    return Err("DOWNLOAD_SIZE_MISMATCH".into());
                }
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                hasher.update(&chunk);
            }
            if size != artifact.size {
                return Err("DOWNLOAD_INCOMPLETE".into());
            }
        }
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        if format!("{:x}", hasher.finalize()) != artifact.sha256 {
            return Err("DOWNLOAD_HASH_MISMATCH: .partial 校验失败，请删除该临时文件后重试".into());
        }
        // Hard link is an atomic no-clobber publication on the same filesystem.
        tokio::fs::hard_link(&partial, &target)
            .await
            .map_err(|e| format!("DOWNLOAD_PUBLISH: {e}"))?;
        tokio::fs::remove_file(partial)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn new(
        address: &str,
        port: u16,
        pem: &str,
        fingerprint: &str,
        token: Option<String>,
    ) -> Result<Self, String> {
        let ip: IpAddr = address.parse().map_err(|_| "INVALID_REMOTE_ADDRESS")?;
        if !tailscale_address(ip) || port == 0 {
            return Err("INVALID_REMOTE_ADDRESS".into());
        }
        Self::build(ip, port, pem, fingerprint, token)
    }
    pub(crate) fn build(
        ip: IpAddr,
        port: u16,
        pem: &str,
        fingerprint: &str,
        token: Option<String>,
    ) -> Result<Self, String> {
        let der = certificate_der(pem)?;
        if hex_digest(&der) != fingerprint {
            return Err("CERTIFICATE_FINGERPRINT_MISMATCH".into());
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(CertificateDer::from(der.clone()))
            .map_err(|_| "INVALID_CERTIFICATE")?;
        let inner = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            provider.clone(),
        )
        .build()
        .map_err(|e| e.to_string())?;
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier {
                certificate: der,
                inner,
            }))
            .with_no_client_auth();
        let client = Client::builder()
            .use_preconfigured_tls(config)
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            base: format!("https://{}", std::net::SocketAddr::new(ip, port)),
            token,
        })
    }
    pub fn connection(connection: &RemoteConnection) -> Result<Self, String> {
        let token = crate::credentials::read(&connection.credential_ref)?;
        Self::new(
            &connection.address,
            connection.port,
            &connection.certificate_pem,
            &connection.fingerprint,
            Some(token),
        )
    }
    pub fn owner(endpoint: &Endpoint) -> Result<Self, String> {
        let token = crate::credentials::read(&endpoint.owner_credential_ref)?;
        Self::new(
            &endpoint.address,
            endpoint.port,
            &endpoint.certificate_pem,
            &endpoint.fingerprint,
            Some(token),
        )
    }
    pub async fn pair(offer: &PairingOffer, name: String) -> Result<PairResponse, String> {
        if offer.version != 1 || offer.session.expires_at <= chrono::Utc::now().timestamp() {
            return Err("PAIRING_EXPIRED_OR_INCOMPATIBLE".into());
        }
        let client = Self::new(
            &offer.address,
            offer.port,
            &offer.certificate_pem,
            &offer.fingerprint,
            None,
        )?;
        client
            .json(
                Method::POST,
                "/v1/pair",
                Some(&PairRequest {
                    session_id: offer.session.id.clone(),
                    secret: offer.session.secret.clone(),
                    controller_name: name,
                }),
            )
            .await
    }
    pub async fn json<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, String> {
        let mut request = self.client.request(method, format!("{}{path}", self.base));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "REMOTE_UNAVAILABLE: 无法连接，或 TLS 身份校验失败；任务状态待同步")?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "REMOTE_RESPONSE_INTERRUPTED")?
        {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("REMOTE_RESPONSE_LIMIT".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let message = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|v| v.get("error").and_then(|v| v.as_str()).map(str::to_owned))
                .unwrap_or_else(|| format!("HTTP_{}", status.as_u16()));
            return Err(message);
        }
        serde_json::from_slice(&bytes).map_err(|_| "INVALID_REMOTE_RESPONSE".into())
    }
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.json::<T, ()>(Method::GET, path, None).await
    }
    pub async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        self.json(Method::POST, path, Some(body)).await
    }
}
