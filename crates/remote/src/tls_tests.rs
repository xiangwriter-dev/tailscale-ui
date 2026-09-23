use crate::{client::RemoteClient, identity::certificate_der};
use axum::{response::Redirect, routing::get, Json, Router};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tailtask_core::remote::{hex_digest, Artifact};

#[tokio::test]
async fn real_tls_checks_pin_address_redirects_and_download_integrity() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let pem = cert.cert.pem();
    let fingerprint = hex_digest(&certificate_der(&pem).unwrap());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let touched = count.clone();
    let task_id = uuid::Uuid::new_v4().to_string();
    let artifact_id = uuid::Uuid::new_v4().to_string();
    let payload = "中文结果文件".as_bytes().to_vec();
    let returned = payload.clone();
    let route = format!("/v1/tasks/{task_id}/artifacts/{artifact_id}");
    let router = Router::new()
        .route(
            "/ok",
            get(|| async { Json(serde_json::json!({"ok":true})) }),
        )
        .route(
            "/redirect",
            get(|| async { Redirect::temporary("/destination") }),
        )
        .route(
            "/destination",
            get(move || {
                let count = touched.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Json(serde_json::json!({"bad":true}))
                }
            }),
        )
        .route(
            &route,
            get(move |headers: axum::http::HeaderMap| {
                let bytes = returned.clone();
                async move {
                    let (start, len, partial) = crate::artifacts::byte_range(
                        headers.get("range").and_then(|v| v.to_str().ok()),
                        bytes.len() as u64,
                    )
                    .unwrap();
                    let mut response =
                        axum::http::Response::builder().status(if partial { 206 } else { 200 });
                    if partial {
                        response = response.header(
                            "content-range",
                            format!("bytes {}-{}/{}", start, start + len - 1, bytes.len()),
                        );
                    }
                    response
                        .body(axum::body::Body::from(
                            bytes[start as usize..(start + len) as usize].to_vec(),
                        ))
                        .unwrap()
                }
            }),
        );
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
        pem.clone().into_bytes(),
        cert.signing_key.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    let handle = axum_server::Handle::new();
    let server = axum_server::from_tcp_rustls(listener, tls)
        .unwrap()
        .handle(handle.clone())
        .serve(router.into_make_service());
    let work = tokio::spawn(server);
    let client =
        RemoteClient::build("127.0.0.1".parse().unwrap(), port, &pem, &fingerprint, None).unwrap();
    assert_eq!(
        client.get::<serde_json::Value>("/ok").await.unwrap()["ok"],
        true
    );
    assert!(client
        .get::<serde_json::Value>("/redirect")
        .await
        .unwrap_err()
        .contains("HTTP_307"));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let changed = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let changed_pem = changed.cert.pem();
    let changed_client = RemoteClient::build(
        "127.0.0.1".parse().unwrap(),
        port,
        &changed_pem,
        &hex_digest(changed.cert.der()),
        None,
    )
    .unwrap();
    assert!(changed_client
        .get::<serde_json::Value>("/ok")
        .await
        .is_err());
    assert!(RemoteClient::new("127.0.0.1", port, &pem, &fingerprint, None).is_err());
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("result.txt");
    let partial = root
        .path()
        .join(format!(".result.txt.{artifact_id}.partial"));
    tokio::fs::write(&partial, &payload[..3]).await.unwrap();
    let artifact = Artifact {
        id: artifact_id,
        task_id,
        name: "result.txt".into(),
        size: payload.len() as u64,
        sha256: hex_digest(&payload),
    };
    client.download(&artifact, &target).await.unwrap();
    assert_eq!(tokio::fs::read(&target).await.unwrap(), payload);
    assert!(!partial.exists());
    assert!(client
        .download(&artifact, &target)
        .await
        .unwrap_err()
        .contains("DESTINATION_EXISTS"));
    let mut corrupt = artifact.clone();
    corrupt.sha256 = "0".repeat(64);
    let bad = root.path().join("bad.txt");
    assert!(client
        .download(&corrupt, &bad)
        .await
        .unwrap_err()
        .contains("HASH_MISMATCH"));
    assert!(!bad.exists());
    handle.shutdown();
    work.await.unwrap().unwrap();
}
