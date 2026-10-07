//! Tests **live** (manuales) contra O2 Cloud.
//!
//! No se ejecutan por defecto: están marcados `#[ignore]` y además exigen
//! `O2CLOUD_LIVE_TESTS=1` y una cookie válida en `O2CLOUD_COOKIE`. Se lanzan a
//! mano con:
//!
//! ```bash
//! O2CLOUD_LIVE_TESTS=1 O2CLOUD_COOKIE='JSESSIONID=…; validationKey=…' \
//!   cargo test --test live -- --ignored --nocapture
//! ```
//!
//! Son **de solo lectura** (status/quota/ls), nunca destructivos.

use o2cloud::auth::Session;
use o2cloud::config::AppConfig;
use o2cloud::{service, O2CloudClient};

/// Construye un cliente live, o `None` si no está configurado.
fn live_client() -> Option<O2CloudClient> {
    if std::env::var("O2CLOUD_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("(live desactivado: falta O2CLOUD_LIVE_TESTS=1)");
        return None;
    }
    let cookie = std::env::var("O2CLOUD_COOKIE").ok()?;
    let config = AppConfig::new(None);
    let session = Session::from_cookie_header(&cookie).ok()?;
    O2CloudClient::new(&config, Some(session)).ok()
}

#[tokio::test]
#[ignore = "live test: requiere O2CLOUD_LIVE_TESTS=1 y O2CLOUD_COOKIE"]
async fn live_status() {
    let Some(client) = live_client() else {
        return;
    };
    let info = service::status(&client).await.expect("status live");
    assert!(info.authenticated);
}

#[tokio::test]
#[ignore = "live test: requiere O2CLOUD_LIVE_TESTS=1 y O2CLOUD_COOKIE"]
async fn live_quota() {
    let Some(client) = live_client() else {
        return;
    };
    let quota = service::quota(&client).await.expect("quota live");
    assert!(quota.quota >= 0);
    eprintln!(
        "quota={} used={} free={} nolimit={}",
        quota.quota, quota.used, quota.free, quota.nolimit
    );
}

#[tokio::test]
#[ignore = "live test: requiere O2CLOUD_LIVE_TESTS=1 y O2CLOUD_COOKIE"]
async fn live_ls_root() {
    let Some(client) = live_client() else {
        return;
    };
    let listing = service::list(&client, "/").await.expect("ls live");
    eprintln!(
        "carpetas={} archivos={}",
        listing.folders.len(),
        listing.files.len()
    );
}
