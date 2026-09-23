use serde::{Deserialize, Serialize};
use std::{
    net::IpAddr,
    path::{Path, PathBuf},
};
use tailtask_core::remote::{
    hex_digest, random_secret, AgentStore, PairingSession, TargetIdentity,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub allowed_directories: Vec<String>,
    pub concurrency: u8,
    pub port: u16,
    pub context_id: String,
    pub node_id: String,
    pub address: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub identity: TargetIdentity,
    pub address: String,
    pub port: u16,
    pub certificate_pem: String,
    pub fingerprint: String,
    pub owner_credential_ref: String,
    pub key_credential_ref: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingOffer {
    pub version: u32,
    pub identity: TargetIdentity,
    pub address: String,
    pub port: u16,
    pub certificate_pem: String,
    pub fingerprint: String,
    pub session: PairingSession,
}

pub fn tailscale_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let b = a.octets();
            b[0] == 100 && (64..=127).contains(&b[1])
        }
        IpAddr::V6(a) => {
            let s = a.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

impl AgentConfig {
    pub async fn validate(&self) -> Result<Vec<PathBuf>, String> {
        if !(1..=4).contains(&self.concurrency)
            || self.port == 0
            || self.allowed_directories.is_empty()
            || self.allowed_directories.len() > 20
        {
            return Err("INVALID_AGENT_CONFIGURATION".into());
        }
        let ip: IpAddr = self.address.parse().map_err(|_| "INVALID_BIND_ADDRESS")?;
        if !tailscale_address(ip) {
            return Err("INVALID_BIND_ADDRESS: 只允许本机 Tailscale 地址".into());
        }
        let snapshot = tailtask_core::tailscale::inspect().await;
        snapshot.require_context(&self.context_id)?;
        if !snapshot
            .devices
            .iter()
            .any(|d| d.is_self && d.node_id == self.node_id && d.addresses.contains(&self.address))
        {
            return Err("LOCAL_NODE_CHANGED".into());
        }
        let mut roots = Vec::new();
        for dir in &self.allowed_directories {
            if !tailtask_core::remote::absolute_for(dir, std::env::consts::OS) {
                return Err("INVALID_ALLOWED_DIRECTORY".into());
            }
            let path = tokio::fs::canonicalize(dir)
                .await
                .map_err(|e| format!("INVALID_ALLOWED_DIRECTORY: {e}"))?;
            if !tokio::fs::metadata(&path)
                .await
                .map_err(|e| e.to_string())?
                .is_dir()
            {
                return Err("INVALID_ALLOWED_DIRECTORY".into());
            }
            roots.push(path);
        }
        Ok(roots)
    }
}

pub async fn cwd_in_roots(cwd: &str, roots: &[PathBuf]) -> Result<PathBuf, String> {
    let path = tokio::fs::canonicalize(cwd)
        .await
        .map_err(|e| format!("WORK_DIRECTORY: {e}"))?;
    if !roots.iter().any(|r| path.starts_with(r))
        || !tokio::fs::metadata(&path)
            .await
            .map_err(|e| e.to_string())?
            .is_dir()
    {
        return Err("DIRECTORY_NOT_ALLOWED".into());
    }
    Ok(path)
}

pub fn certificate_der(pem: &str) -> Result<Vec<u8>, String> {
    if pem.len() > 16384 {
        return Err("CERTIFICATE_LIMIT".into());
    }
    let certs = rustls_pemfile::certs(&mut std::io::Cursor::new(pem.as_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "INVALID_CERTIFICATE")?;
    if certs.len() != 1 {
        return Err("INVALID_CERTIFICATE".into());
    }
    Ok(certs[0].to_vec())
}

pub async fn initialize(
    store: &AgentStore,
    config: &AgentConfig,
    dir: &Path,
) -> Result<Endpoint, String> {
    if let Some(saved) = store.setting("remote_identity").await? {
        let e: Endpoint = serde_json::from_str(&saved).map_err(|e| e.to_string())?;
        if e.identity.node_id != config.node_id
            || e.address != config.address
            || e.port != config.port
        {
            return Err(
                "AGENT_IDENTITY_CHANGED: 请停止执行端并重新配置身份；已有授权不能用于其他节点"
                    .into(),
            );
        }
        crate::credentials::read(&e.key_credential_ref)?;
        crate::credentials::read(&e.owner_credential_ref)?;
        write_json(&dir.join("endpoint.json"), &e).await?;
        return Ok(e);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let certified = rcgen::generate_simple_self_signed(vec![config.address.clone()])
        .map_err(|e| e.to_string())?;
    let key_ref = format!("agent-key-{id}");
    let owner_ref = format!("agent-owner-{id}");
    let owner_secret = random_secret()?;
    crate::credentials::save(&key_ref, &certified.signing_key.serialize_pem())?;
    if let Err(e) = crate::credentials::save(&owner_ref, &owner_secret) {
        let _ = crate::credentials::remove(&key_ref);
        return Err(e);
    }
    let endpoint = Endpoint {
        identity: TargetIdentity {
            agent_id: id,
            node_id: config.node_id.clone(),
        },
        address: config.address.clone(),
        port: config.port,
        certificate_pem: certified.cert.pem(),
        fingerprint: hex_digest(certified.cert.der()),
        owner_credential_ref: owner_ref,
        key_credential_ref: key_ref,
    };
    store
        .set_setting("owner_token_hash", &hex_digest(owner_secret.as_bytes()))
        .await?;
    store
        .set_setting(
            "remote_identity",
            &serde_json::to_string(&endpoint).map_err(|e| e.to_string())?,
        )
        .await?;
    write_json(&dir.join("endpoint.json"), &endpoint).await?;
    Ok(endpoint)
}

pub async fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temp).await.map_err(|e| e.to_string())?;
    use tokio::io::AsyncWriteExt;
    file.write_all(&bytes).await.map_err(|e| e.to_string())?;
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::rename(temp, path)
        .await
        .map_err(|e| e.to_string())
}

pub async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("CONFIGURATION_LIMIT".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("INVALID_CONFIGURATION: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_tailnet_addresses_are_valid_bind_candidates() {
        for s in [
            "0.0.0.0",
            "127.0.0.1",
            "192.168.1.1",
            "100.63.0.1",
            "100.128.0.1",
            "::",
            "::1",
        ] {
            assert!(!tailscale_address(s.parse().unwrap()));
        }
        for s in ["100.64.0.1", "100.127.255.254", "fd7a:115c:a1e0::1"] {
            assert!(tailscale_address(s.parse().unwrap()));
        }
    }
    #[tokio::test]
    async fn work_roots_use_canonical_components() {
        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        tokio::fs::create_dir(&allowed).await.unwrap();
        let sibling = dir.path().join("allowed-other");
        tokio::fs::create_dir(&sibling).await.unwrap();
        let roots = vec![tokio::fs::canonicalize(&allowed).await.unwrap()];
        assert!(cwd_in_roots(allowed.to_str().unwrap(), &roots)
            .await
            .is_ok());
        assert!(cwd_in_roots(sibling.to_str().unwrap(), &roots)
            .await
            .is_err());
        assert!(cwd_in_roots(allowed.join("..").to_str().unwrap(), &roots)
            .await
            .is_err());
    }
}
