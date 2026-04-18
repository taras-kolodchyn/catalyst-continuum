use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

const RECEIPT_VERSION: u32 = 1;
const GITHUB_PROVIDER: &str = "github";

#[derive(Debug, Clone)]
pub struct GitHubWebhookHeaders {
    pub event: String,
    pub delivery_id: String,
    pub signature_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitHubWebhookErrorKind {
    BadRequest,
    Unauthorized,
    ServiceUnavailable,
    Internal,
}

#[derive(Debug, Clone)]
pub struct GitHubWebhookError {
    kind: GitHubWebhookErrorKind,
    message: String,
}

impl GitHubWebhookError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            kind: GitHubWebhookErrorKind::BadRequest,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            kind: GitHubWebhookErrorKind::Unauthorized,
            message: message.into(),
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            kind: GitHubWebhookErrorKind::ServiceUnavailable,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: GitHubWebhookErrorKind::Internal,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> GitHubWebhookErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitHubWebhookRepository {
    pub full_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitHubWebhookReceiptSummary {
    pub status: &'static str,
    pub outcome: &'static str,
    pub provider: &'static str,
    pub delivery_id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<GitHubWebhookRepository>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation_id: Option<u64>,
    pub payload_digest: String,
    pub payload_bytes: usize,
    pub signature_verified: bool,
    pub received_at_epoch_ms: u64,
    pub receipt_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
struct PersistedGitHubWebhookHeaders {
    event: String,
    delivery_id: String,
    signature_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
struct PersistedGitHubWebhookReceipt {
    receipt_version: u32,
    summary: GitHubWebhookReceiptSummary,
    headers: PersistedGitHubWebhookHeaders,
    payload: Value,
}

pub fn ingest_github_webhook(
    headers: &GitHubWebhookHeaders,
    body: &[u8],
    artifact_root: &Path,
    webhook_secret: Option<&str>,
) -> std::result::Result<GitHubWebhookReceiptSummary, GitHubWebhookError> {
    let event = normalize_required_header(headers.event.as_str(), "X-GitHub-Event")?;
    let delivery_id = normalize_delivery_id(headers.delivery_id.as_str())?;
    let signature_sha256 =
        normalize_required_header(headers.signature_sha256.as_str(), "X-Hub-Signature-256")?;
    let secret = webhook_secret
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            GitHubWebhookError::service_unavailable(
                "GitHub App webhook secret is not configured for this instance",
            )
        })?;

    verify_signature(signature_sha256, body, secret)?;

    let payload: Value = serde_json::from_slice(body).map_err(|error| {
        GitHubWebhookError::bad_request(format!(
            "failed to parse GitHub webhook JSON payload: {error}"
        ))
    })?;
    if !payload.is_object() {
        return Err(GitHubWebhookError::bad_request(
            "GitHub webhook payload must be a JSON object",
        ));
    }

    let action = payload
        .get("action")
        .and_then(Value::as_str)
        .map(str::to_string);
    let repository = extract_repository(&payload);
    let installation_id = payload
        .get("installation")
        .and_then(|installation| installation.get("id"))
        .and_then(Value::as_u64);
    let received_at_epoch_ms = current_epoch_millis().map_err(|error| {
        GitHubWebhookError::internal(format!("failed to compute receipt timestamp: {error}"))
    })?;
    let payload_digest = format!("sha256:{}", sha256_hex(body));
    let outcome = if event == "ping" { "ping" } else { "accepted" };
    let message = if event == "ping" {
        ping_message(&payload).unwrap_or_else(|| "validated GitHub App ping delivery".to_string())
    } else {
        format!("accepted GitHub webhook delivery `{event}`")
    };

    let receipt_path = receipt_path(artifact_root, delivery_id);
    let summary = GitHubWebhookReceiptSummary {
        status: "accepted",
        outcome,
        provider: GITHUB_PROVIDER,
        delivery_id: delivery_id.to_string(),
        event: event.to_string(),
        action,
        repository,
        installation_id,
        payload_digest,
        payload_bytes: body.len(),
        signature_verified: true,
        received_at_epoch_ms,
        receipt_path: receipt_path.display().to_string(),
        message,
    };
    persist_receipt(&summary, headers, &payload, &receipt_path).map_err(|error| {
        GitHubWebhookError::internal(format!("failed to persist GitHub webhook receipt: {error}"))
    })?;

    Ok(summary)
}

fn normalize_required_header<'a>(
    value: &'a str,
    header_name: &str,
) -> std::result::Result<&'a str, GitHubWebhookError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GitHubWebhookError::bad_request(format!(
            "{header_name} header is required"
        )));
    }

    Ok(trimmed)
}

fn normalize_delivery_id(value: &str) -> std::result::Result<&str, GitHubWebhookError> {
    let trimmed = normalize_required_header(value, "X-GitHub-Delivery")?;
    if !trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(GitHubWebhookError::bad_request(
            "X-GitHub-Delivery contains unsupported characters",
        ));
    }

    Ok(trimmed)
}

fn verify_signature(
    signature_header: &str,
    body: &[u8],
    webhook_secret: &str,
) -> std::result::Result<(), GitHubWebhookError> {
    let expected_signature = signature_header.strip_prefix("sha256=").ok_or_else(|| {
        GitHubWebhookError::bad_request(
            "X-Hub-Signature-256 must use the sha256=<hex-digest> format",
        )
    })?;
    let expected_signature = decode_hex(expected_signature).map_err(|error| {
        GitHubWebhookError::bad_request(format!(
            "X-Hub-Signature-256 is not valid hexadecimal: {error}"
        ))
    })?;

    let mut mac = HmacSha256::new_from_slice(webhook_secret.as_bytes()).map_err(|error| {
        GitHubWebhookError::internal(format!(
            "failed to initialize GitHub webhook signature verifier: {error}"
        ))
    })?;
    mac.update(body);
    mac.verify_slice(&expected_signature).map_err(|_| {
        GitHubWebhookError::unauthorized("GitHub webhook signature verification failed")
    })
}

fn extract_repository(payload: &Value) -> Option<GitHubWebhookRepository> {
    let repository = payload.get("repository")?.as_object()?;
    let full_name = repository.get("full_name")?.as_str()?.to_string();
    let default_branch = repository
        .get("default_branch")
        .and_then(Value::as_str)
        .map(str::to_string);

    Some(GitHubWebhookRepository {
        full_name,
        default_branch,
    })
}

fn ping_message(payload: &Value) -> Option<String> {
    if payload.get("zen").and_then(Value::as_str).is_some() {
        let zen = payload.get("zen").and_then(Value::as_str)?;
        return Some(format!("validated GitHub App ping delivery: {zen}"));
    }
    None
}

fn receipt_path(artifact_root: &Path, delivery_id: &str) -> PathBuf {
    artifact_root
        .join("github-webhooks")
        .join(delivery_id)
        .join("receipt.json")
}

fn persist_receipt(
    summary: &GitHubWebhookReceiptSummary,
    headers: &GitHubWebhookHeaders,
    payload: &Value,
    receipt_path: &Path,
) -> Result<()> {
    let receipt = PersistedGitHubWebhookReceipt {
        receipt_version: RECEIPT_VERSION,
        summary: summary.clone(),
        headers: PersistedGitHubWebhookHeaders {
            event: headers.event.clone(),
            delivery_id: headers.delivery_id.clone(),
            signature_sha256: headers.signature_sha256.clone(),
        },
        payload: payload.clone(),
    };
    let receipt_bytes =
        serde_json::to_vec_pretty(&receipt).context("failed to serialize webhook receipt")?;
    let receipt_dir = receipt_path
        .parent()
        .context("receipt path should always have a parent directory")?;
    fs::create_dir_all(receipt_dir).with_context(|| {
        format!(
            "failed to create GitHub webhook receipt directory: {}",
            receipt_dir.display()
        )
    })?;
    fs::write(receipt_path, receipt_bytes).with_context(|| {
        format!(
            "failed to write GitHub webhook receipt file: {}",
            receipt_path.display()
        )
    })?;

    Ok(())
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        anyhow::bail!("hex digest length must be even");
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.chars();
    while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
        let high = hex_nibble(high)?;
        let low = hex_nibble(low)?;
        bytes.push((high << 4) | low);
    }

    Ok(bytes)
}

fn hex_nibble(value: char) -> Result<u8> {
    match value {
        '0'..='9' => Ok((value as u8) - b'0'),
        'a'..='f' => Ok((value as u8) - b'a' + 10),
        'A'..='F' => Ok((value as u8) - b'A' + 10),
        _ => anyhow::bail!("invalid hex character `{value}`"),
    }
}

fn sha256_hex(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn current_epoch_millis() -> Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?;
    u64::try_from(duration.as_millis()).context("epoch millisecond timestamp exceeds u64 range")
}

#[cfg(test)]
mod tests {
    use super::{GitHubWebhookErrorKind, GitHubWebhookHeaders, decode_hex, ingest_github_webhook};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use std::fs;

    type HmacSha256 = Hmac<Sha256>;

    #[test]
    fn accepts_ping_delivery_and_persists_receipt() {
        let body =
            br#"{"zen":"Keep it logically awesome.","hook_id":42,"repository":{"full_name":"smartit/catalyst-continuum","default_branch":"main"}}"#;
        let secret = "continuum-webhook-secret";
        let signature = signature_for(secret, body);
        let artifact_root =
            std::env::temp_dir().join(format!("continuum-webhook-{}", uuid::Uuid::new_v4()));

        let summary = ingest_github_webhook(
            &GitHubWebhookHeaders {
                event: "ping".to_string(),
                delivery_id: "11111111-1111-1111-1111-111111111111".to_string(),
                signature_sha256: signature,
            },
            body,
            &artifact_root,
            Some(secret),
        )
        .expect("ping delivery should be accepted");

        assert_eq!(summary.outcome, "ping");
        assert_eq!(summary.event, "ping");
        assert!(
            summary
                .message
                .contains("validated GitHub App ping delivery")
        );
        assert!(summary.signature_verified);

        let receipt_path = std::path::PathBuf::from(&summary.receipt_path);
        assert!(receipt_path.is_file());

        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt_path).expect("receipt should be persisted"))
                .expect("persisted receipt should be valid JSON");
        assert_eq!(receipt["summary"]["event"], "ping");
        assert_eq!(
            receipt["payload"]["repository"]["full_name"],
            "smartit/catalyst-continuum"
        );

        let _ = fs::remove_dir_all(artifact_root);
    }

    #[test]
    fn rejects_invalid_signature() {
        let error = ingest_github_webhook(
            &GitHubWebhookHeaders {
                event: "ping".to_string(),
                delivery_id: "11111111-1111-1111-1111-111111111111".to_string(),
                signature_sha256: "sha256=0000".to_string(),
            },
            br#"{"zen":"hi"}"#,
            std::path::Path::new(".tmp"),
            Some("continuum-webhook-secret"),
        )
        .expect_err("invalid signature should be rejected");

        assert_eq!(error.kind(), GitHubWebhookErrorKind::Unauthorized);
        assert!(error.message().contains("signature verification failed"));
    }

    #[test]
    fn rejects_missing_secret_configuration() {
        let error = ingest_github_webhook(
            &GitHubWebhookHeaders {
                event: "push".to_string(),
                delivery_id: "11111111-1111-1111-1111-111111111111".to_string(),
                signature_sha256: signature_for("continuum-webhook-secret", br#"{}"#),
            },
            br#"{}"#,
            std::path::Path::new(".tmp"),
            None,
        )
        .expect_err("missing secret should be rejected");

        assert_eq!(error.kind(), GitHubWebhookErrorKind::ServiceUnavailable);
        assert!(error.message().contains("webhook secret is not configured"));
    }

    #[test]
    fn rejects_invalid_delivery_id_characters() {
        let error = ingest_github_webhook(
            &GitHubWebhookHeaders {
                event: "push".to_string(),
                delivery_id: "../escape".to_string(),
                signature_sha256: signature_for("continuum-webhook-secret", br#"{}"#),
            },
            br#"{}"#,
            std::path::Path::new(".tmp"),
            Some("continuum-webhook-secret"),
        )
        .expect_err("invalid delivery ids should be rejected");

        assert_eq!(error.kind(), GitHubWebhookErrorKind::BadRequest);
        assert!(error.message().contains("unsupported characters"));
    }

    #[test]
    fn decodes_hex_digests() {
        let decoded = decode_hex("0a10ff").expect("hex should decode");
        assert_eq!(decoded, vec![0x0a, 0x10, 0xff]);
    }

    fn signature_for(secret: &str, body: &[u8]) -> String {
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key should initialize");
        mac.update(body);
        let signature = mac.finalize().into_bytes();
        format!(
            "sha256={}",
            signature
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }
}
