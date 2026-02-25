//! Public API surface for flutter_rust_bridge codegen.
//!
//! Every `pub` function and type in this module is picked up by FRB and
//! gets a corresponding Dart binding generated automatically.
//!
//! ## Streaming pattern
//!
//! FRB v2 generates `StreamSink` in user boilerplate code rather than
//! exporting it from the crate.  To avoid depending on generated code,
//! subscriptions use an **async pull model**: call `dart_subscription_next`
//! in a loop from Dart, which internally awaits the next event from the
//! underlying `SubscriptionManager`.

use crate::models::{
    DartAuthProvider, DartChangeEvent, DartHealthCheckResponse, DartLoginResponse,
    DartQueryResponse, DartServerSetupRequest, DartServerSetupResponse, DartSetupStatusResponse,
    DartSubscriptionConfig,
};
use flutter_rust_bridge::frb;
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Client wrapper
// ---------------------------------------------------------------------------

/// Opaque handle to a connected KalamDB client.
///
/// Create one via [`dart_create_client`] and pass it to query/subscribe helpers.
pub struct DartKalamClient {
    inner: kalam_link::KalamLinkClient,
}

/// Create a new KalamDB client.
///
/// * `base_url` — server URL, e.g. `"https://db.example.com"` or `"http://localhost:3000"`.
/// * `auth` — authentication method (basic, JWT, or none).
/// * `timeout_ms` — optional HTTP request timeout in milliseconds (default 30 000).
/// * `max_retries` — optional retry count for idempotent queries (default 3).
#[frb(sync)]
pub fn dart_create_client(
    base_url: String,
    auth: DartAuthProvider,
    timeout_ms: Option<i64>,
    max_retries: Option<i32>,
) -> anyhow::Result<DartKalamClient> {
    let mut builder = kalam_link::KalamLinkClient::builder()
        .base_url(base_url)
        .auth(auth.into_native());

    if let Some(ms) = timeout_ms {
        builder = builder.timeout(std::time::Duration::from_millis(ms as u64));
    }
    if let Some(r) = max_retries {
        builder = builder.max_retries(r as u32);
    }

    let client = builder.build()?;
    Ok(DartKalamClient { inner: client })
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

/// Execute a SQL query, optionally with parameters and namespace.
///
/// * `params_json` — JSON-encoded array of parameter values, e.g. `'["val1", 42]'`.
///   Pass `null` for no parameters.
/// * `namespace` — optional namespace context for unqualified table names.
pub async fn dart_execute_query(
    client: &DartKalamClient,
    sql: String,
    params_json: Option<String>,
    namespace: Option<String>,
) -> anyhow::Result<DartQueryResponse> {
    let params: Option<Vec<serde_json::Value>> = match params_json {
        Some(json) => Some(serde_json::from_str(&json)?),
        None => None,
    };
    let response = client
        .inner
        .execute_query(&sql, None, params, namespace.as_deref())
        .await?;
    Ok(DartQueryResponse::from(response))
}

// ---------------------------------------------------------------------------
// Auth endpoints
// ---------------------------------------------------------------------------

/// Log in with username and password. Returns tokens and user info.
pub async fn dart_login(
    client: &DartKalamClient,
    username: String,
    password: String,
) -> anyhow::Result<DartLoginResponse> {
    let response = client.inner.login(&username, &password).await?;
    Ok(DartLoginResponse::from(response))
}

/// Refresh an access token using a refresh token.
pub async fn dart_refresh_token(
    client: &DartKalamClient,
    refresh_token: String,
) -> anyhow::Result<DartLoginResponse> {
    let response = client.inner.refresh_access_token(&refresh_token).await?;
    Ok(DartLoginResponse::from(response))
}

// ---------------------------------------------------------------------------
// Health / Setup
// ---------------------------------------------------------------------------

/// Check server health (version, status, etc.).
pub async fn dart_health_check(
    client: &DartKalamClient,
) -> anyhow::Result<DartHealthCheckResponse> {
    let response = client.inner.health_check().await?;
    Ok(DartHealthCheckResponse::from(response))
}

/// Check whether the server requires initial setup.
pub async fn dart_check_setup_status(
    client: &DartKalamClient,
) -> anyhow::Result<DartSetupStatusResponse> {
    let response = client.inner.check_setup_status().await?;
    Ok(DartSetupStatusResponse::from(response))
}

/// Perform initial server setup (create first admin user).
pub async fn dart_server_setup(
    client: &DartKalamClient,
    request: DartServerSetupRequest,
) -> anyhow::Result<DartServerSetupResponse> {
    let response = client.inner.server_setup(request.into_native()).await?;
    Ok(DartServerSetupResponse::from(response))
}

// ---------------------------------------------------------------------------
// Subscription (async pull model)
// ---------------------------------------------------------------------------

/// Opaque handle to an active live-query subscription.
///
/// On the Dart side, call [`dart_subscription_next`] in a loop to pull
/// events. The loop ends when `None` is returned (subscription closed).
pub struct DartSubscription {
    inner: Arc<Mutex<kalam_link::SubscriptionManager>>,
    sub_id: String,
}

/// Create a live-query subscription.
///
/// * `sql` — the SELECT query to subscribe to.
/// * `config` — optional advanced configuration (batch size, etc.).
///
/// Returns an opaque [`DartSubscription`] handle.  Use
/// [`dart_subscription_next`] to pull events and
/// [`dart_subscription_close`] to tear down.
pub async fn dart_subscribe(
    client: &DartKalamClient,
    sql: String,
    config: Option<DartSubscriptionConfig>,
) -> anyhow::Result<DartSubscription> {
    let sub = if let Some(cfg) = config {
        let mut native_cfg = cfg.into_native();
        native_cfg.sql = sql;
        client.inner.subscribe_with_config(native_cfg).await?
    } else {
        client.inner.subscribe(&sql).await?
    };

    let sub_id = sub.subscription_id().to_owned();
    Ok(DartSubscription {
        inner: Arc::new(Mutex::new(sub)),
        sub_id,
    })
}

/// Pull the next change event from a subscription.
///
/// Returns `None` when the subscription has ended (server closed or
/// [`dart_subscription_close`] was called).
pub async fn dart_subscription_next(
    subscription: &DartSubscription,
) -> anyhow::Result<Option<DartChangeEvent>> {
    let mut sub = subscription.inner.lock().await;
    match sub.next().await {
        Some(Ok(event)) => Ok(Some(DartChangeEvent::from(event))),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Close a subscription and release server-side resources.
pub async fn dart_subscription_close(subscription: &DartSubscription) -> anyhow::Result<()> {
    let mut sub = subscription.inner.lock().await;
    sub.close().await?;
    Ok(())
}

/// Get the server-assigned subscription ID.
#[frb(sync)]
pub fn dart_subscription_id(subscription: &DartSubscription) -> String {
    subscription.sub_id.clone()
}
