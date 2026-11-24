use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Method;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::Sha256;

use database::StarfishRepository;

use crate::{
    error::ApiError,
    state::{AppState, PayNowConfig, StarfishConfig},
};

use super::{rate_limit, require_starfish};

const PAYNOW_STORES_URL: &str = "https://api.paynow.gg/v1/stores";
const RETURN_URL: &str = "https://urchin.gg/starfish/purchased";
const CANCEL_URL: &str = "https://urchin.gg/starfish";
const SIGNATURE_TOLERANCE_MS: i64 = 5 * 60 * 1000;
const CHECKOUTS_PER_WINDOW: i64 = 10;
/// The Starfish terms buyers agree to; matches `TERMS_VERSION` on the website.
const TERMS_VERSION: &str = "2026-11-24";

pub fn router() -> Router<AppState> {
    Router::new().route("/paynow/webhook", post(receive_webhook))
}

#[derive(Deserialize)]
struct PayNowCustomer {
    id: String,
}

#[derive(Deserialize)]
struct PayNowCheckout {
    url: String,
}

#[derive(Deserialize)]
struct WebhookEvent {
    event_type: String,
    body: Value,
}

#[derive(Deserialize)]
struct Order {
    id: String,
    currency: String,
    total_amount: i64,
    checkout: Option<OrderCheckout>,
    lines: Vec<OrderLine>,
}

#[derive(Deserialize)]
struct OrderCheckout {
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Deserialize)]
struct OrderLine {
    product_id: String,
}

#[derive(Deserialize)]
struct Refund {
    order_id: String,
    status: Option<String>,
}

#[derive(Deserialize)]
struct ChargebackStep {
    order_id: String,
    transition: String,
    sequence: i64,
}

/// Creates a PayNow checkout for one Starfish license and returns the page to send the buyer to.
pub(crate) async fn create_checkout(
    state: &AppState,
    discord_id: i64,
    terms_version: &str,
) -> Result<String, ApiError> {
    if terms_version != TERMS_VERSION {
        return Err(ApiError::BadRequest("terms_outdated".into()));
    }
    let starfish = require_starfish(state)?;
    let paynow = require_paynow(&starfish)?;
    rate_limit(
        state,
        &format!("sf:checkout:{discord_id}"),
        CHECKOUTS_PER_WINDOW,
    )
    .await?;

    let repo = StarfishRepository::new(state.db.pool());
    let existing = repo.get_user_by_discord_id(discord_id).await?;
    if existing.is_some_and(|user| user.license_status == "active") {
        return Err(ApiError::Conflict("already_licensed".into()));
    }
    repo.record_terms_acceptance(discord_id, TERMS_VERSION)
        .await?;

    let customer_id = paynow_customer_id(paynow, discord_id).await?;
    let checkout: PayNowCheckout = paynow_json(
        paynow_request(paynow, Method::POST, "checkouts").json(&json!({
            "customer_id": customer_id,
            "lines": [{ "product_id": paynow.product_id, "quantity": 1 }],
            "metadata": {
                "discord_id": discord_id.to_string(),
                "terms_version": TERMS_VERSION,
            },
            "return_url": RETURN_URL,
            "cancel_url": CANCEL_URL,
        })),
    )
    .await?;

    Ok(checkout.url)
}

async fn receive_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let starfish = require_starfish(&state)?;
    let paynow = require_paynow(&starfish)?;
    verify_signature(
        &paynow.webhook_secret,
        &headers,
        &body,
        Utc::now().timestamp_millis(),
    )?;

    let event: WebhookEvent = parse(&body)?;
    let repo = StarfishRepository::new(state.db.pool());
    match event.event_type.as_str() {
        "ON_ORDER_COMPLETED" => complete_order(&repo, paynow, parse_value(event.body)?).await?,
        "ON_REFUND" => refund_order(&repo, parse_value(event.body)?).await?,
        "ON_CHARGEBACK_LIFECYCLE" => apply_chargeback(&repo, parse_value(event.body)?).await?,
        other => tracing::debug!(event_type = other, "ignoring PayNow event"),
    }

    Ok(StatusCode::NO_CONTENT)
}

fn require_paynow(starfish: &Arc<StarfishConfig>) -> Result<&PayNowConfig, ApiError> {
    starfish
        .paynow
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("Purchases not configured".into()))
}

async fn complete_order(
    repo: &StarfishRepository<'_>,
    paynow: &PayNowConfig,
    order: Order,
) -> Result<(), ApiError> {
    if !order
        .lines
        .iter()
        .any(|line| line.product_id == paynow.product_id)
    {
        return Ok(());
    }

    let discord_id = order
        .checkout
        .as_ref()
        .and_then(|checkout| checkout.metadata.get("discord_id"))
        .and_then(|id| id.parse::<i64>().ok());
    let Some(discord_id) = discord_id else {
        tracing::error!(order_id = %order.id, "Starfish order has no Discord ID; grant its license by hand");
        return Ok(());
    };

    if repo
        .record_paid_order(&order.id, discord_id, order.total_amount, &order.currency)
        .await?
    {
        tracing::info!(order_id = %order.id, discord_id, "Starfish license purchased");
    }
    Ok(())
}

async fn refund_order(repo: &StarfishRepository<'_>, refund: Refund) -> Result<(), ApiError> {
    if matches!(refund.status.as_deref(), Some("canceled" | "failed")) {
        return Ok(());
    }

    if repo.refund_order(&refund.order_id).await? {
        tracing::info!(order_id = %refund.order_id, "Starfish order refunded");
    }
    Ok(())
}

async fn apply_chargeback(
    repo: &StarfishRepository<'_>,
    step: ChargebackStep,
) -> Result<(), ApiError> {
    let disputed = match step.transition.as_str() {
        "opened" => true,
        "won" | "prevented" => false,
        _ => return Ok(()),
    };

    if repo
        .apply_chargeback(&step.order_id, step.sequence, disputed)
        .await?
    {
        tracing::info!(order_id = %step.order_id, transition = %step.transition, "Starfish chargeback applied");
    }
    Ok(())
}

async fn paynow_customer_id(paynow: &PayNowConfig, discord_id: i64) -> Result<String, ApiError> {
    let name = discord_id.to_string();
    let lookup = paynow_request(paynow, Method::GET, "customers/lookup")
        .query(&[("name", &name)])
        .send()
        .await
        .map_err(paynow_error)?;
    if lookup.status() != reqwest::StatusCode::NOT_FOUND {
        return read_paynow_json::<PayNowCustomer>(lookup)
            .await
            .map(|c| c.id);
    }

    let created: PayNowCustomer = paynow_json(
        paynow_request(paynow, Method::POST, "customers")
            .json(&json!({ "name": name, "metadata": { "discord_id": name } })),
    )
    .await?;
    Ok(created.id)
}

fn paynow_request(paynow: &PayNowConfig, method: Method, path: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .request(
            method,
            format!("{PAYNOW_STORES_URL}/{}/{path}", paynow.store_id),
        )
        .header("Authorization", format!("APIKey {}", paynow.api_key))
}

async fn paynow_json<T: DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T, ApiError> {
    read_paynow_json(request.send().await.map_err(paynow_error)?).await
}

async fn read_paynow_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, ApiError> {
    response
        .error_for_status()
        .map_err(paynow_error)?
        .json()
        .await
        .map_err(paynow_error)
}

fn paynow_error(error: reqwest::Error) -> ApiError {
    ApiError::Upstream(format!("PayNow API error: {error}"))
}

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body)
        .map_err(|e| ApiError::BadRequest(format!("invalid PayNow payload: {e}")))
}

fn parse_value<T: DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|e| ApiError::BadRequest(format!("invalid PayNow payload: {e}")))
}

fn verify_signature(
    secret: &str,
    headers: &HeaderMap,
    body: &[u8],
    now_ms: i64,
) -> Result<(), ApiError> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized(format!("missing {name}")))
    };
    let timestamp = header("PayNow-Timestamp")?;
    let signature = BASE64
        .decode(header("PayNow-Signature")?)
        .map_err(|_| ApiError::Unauthorized("invalid_signature".into()))?;

    let sent_ms: i64 = timestamp
        .parse()
        .map_err(|_| ApiError::Unauthorized("invalid_timestamp".into()))?;
    if (now_ms - sent_ms).abs() > SIGNATURE_TOLERANCE_MS {
        return Err(ApiError::Unauthorized("stale_timestamp".into()));
    }

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&signature)
        .map_err(|_| ApiError::Unauthorized("invalid_signature".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";
    const SENT_MS: i64 = 1_700_000_000_000;
    const BODY: &[u8] = br#"{"event_type":"ON_REFUND"}"#;
    const SIGNATURE: &str = "A3gh0qy2Jb161hsqqkRQCtC7QoIQ+c3BxTf4okYNFWA=";

    fn signed_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("PayNow-Timestamp", SENT_MS.to_string().parse().unwrap());
        headers.insert("PayNow-Signature", SIGNATURE.parse().unwrap());
        headers
    }

    #[test]
    fn accepts_a_valid_signature() {
        assert!(verify_signature(SECRET, &signed_headers(), BODY, SENT_MS + 1_000).is_ok());
    }

    #[test]
    fn rejects_a_tampered_body() {
        let tampered = br#"{"event_type":"ON_ORDER_COMPLETED"}"#;
        assert!(verify_signature(SECRET, &signed_headers(), tampered, SENT_MS).is_err());
    }

    #[test]
    fn rejects_a_stale_timestamp() {
        let late = SENT_MS + SIGNATURE_TOLERANCE_MS + 1;
        assert!(verify_signature(SECRET, &signed_headers(), BODY, late).is_err());
    }

    #[test]
    fn rejects_a_wrong_secret() {
        assert!(verify_signature("other", &signed_headers(), BODY, SENT_MS).is_err());
    }
}
