//! Hub API client for fetching remote runbooks
//!
//! This module provides functionality to fetch runbook content from Atuin Hub.

use serde::Deserialize;

const HUB_API_BASE: &str = "https://hub.atuin.sh/api";

/// A snapshot from the hub API
#[derive(Debug, Deserialize)]
pub struct HubSnapshot {
    pub id: String,
    pub tag: String,
    pub content: Vec<serde_json::Value>,
}

/// A runbook from the hub API
#[derive(Debug, Deserialize)]
pub struct HubRunbook {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub nwo: String,
    pub visibility: String,
    #[serde(default)]
    pub snapshots: Vec<HubSnapshotMeta>,
    /// Content is only present when fetched with include=content or via resolve
    #[serde(default)]
    pub content: Option<Vec<serde_json::Value>>,
}

/// Snapshot metadata (without content)
#[derive(Debug, Deserialize)]
pub struct HubSnapshotMeta {
    pub id: String,
    pub tag: String,
}

/// Response wrapper for runbook endpoint
#[derive(Debug, Deserialize)]
struct RunbookResponse {
    runbook: HubRunbook,
}

/// Response wrapper for snapshot endpoint
#[derive(Debug, Deserialize)]
struct SnapshotResponse {
    snapshot: HubSnapshot,
}

/// Response wrapper for resolve endpoint
#[derive(Debug, Deserialize)]
struct ResolveResponse {
    runbook: HubRunbook,
    snapshot: Option<HubSnapshot>,
}

/// Parsed URI reference: "user/runbook" or "user/runbook:tag"
#[derive(Debug, Clone)]
pub struct ParsedUri {
    pub nwo: String,
    pub tag: Option<String>,
}

impl ParsedUri {
    /// Parse a URI string like "user/runbook" or "user/runbook:tag"
    pub fn parse(uri: &str) -> Option<Self> {
        // Strip optional hub.atuin.sh prefix
        let uri = uri
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("hub.atuin.sh/");

        // Split on colon to get tag
        let (nwo, tag) = if let Some((nwo, tag)) = uri.split_once(':') {
            (nwo.to_string(), Some(tag.to_string()))
        } else {
            (uri.to_string(), None)
        };

        // Validate nwo format (should be "user/slug")
        if !nwo.contains('/') || nwo.starts_with('/') || nwo.ends_with('/') {
            return None;
        }

        Some(Self { nwo, tag })
    }
}

/// Hub API client
pub struct HubClient {
    client: reqwest::Client,
    base_url: String,
}

impl HubClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: HUB_API_BASE.to_string(),
        }
    }

    /// Fetch a runbook by ID
    pub async fn get_runbook_by_id(&self, id: &str) -> Result<HubRunbook, HubError> {
        let url = format!(
            "{}/runbooks/{}?include=snapshots,content",
            self.base_url, id
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| HubError::NetworkError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(HubError::NotFound(id.to_string()));
        }

        if !response.status().is_success() {
            return Err(HubError::ApiError(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let resp: RunbookResponse = response
            .json()
            .await
            .map_err(|e| HubError::ParseError(e.to_string()))?;

        Ok(resp.runbook)
    }

    /// Resolve a runbook by NWO (name-with-owner), optionally with a tag
    ///
    /// This uses the /resolve/runbook endpoint which returns both runbook metadata
    /// and snapshot content in one request.
    pub async fn resolve_by_nwo(
        &self,
        nwo: &str,
        tag: Option<&str>,
    ) -> Result<(HubRunbook, Option<HubSnapshot>), HubError> {
        let url = format!("{}/resolve/runbook", self.base_url);

        let mut request = self
            .client
            .get(&url)
            .query(&[("nwo", nwo), ("with_content", "true")]);

        if let Some(tag) = tag {
            request = request.query(&[("tag", tag)]);
        }

        let response = request
            .send()
            .await
            .map_err(|e| HubError::NetworkError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(HubError::NotFound(nwo.to_string()));
        }

        if !response.status().is_success() {
            return Err(HubError::ApiError(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let resp: ResolveResponse = response
            .json()
            .await
            .map_err(|e| HubError::ParseError(e.to_string()))?;

        Ok((resp.runbook, resp.snapshot))
    }

    /// Fetch a snapshot by ID
    pub async fn get_snapshot(&self, id: &str) -> Result<HubSnapshot, HubError> {
        let url = format!("{}/snapshots/{}", self.base_url, id);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| HubError::NetworkError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(HubError::NotFound(id.to_string()));
        }

        if !response.status().is_success() {
            return Err(HubError::ApiError(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let resp: SnapshotResponse = response
            .json()
            .await
            .map_err(|e| HubError::ParseError(e.to_string()))?;

        Ok(resp.snapshot)
    }
}

impl Default for HubClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Load runbook from a hub URI (user/runbook or user/runbook:tag)
///
/// This is a convenience function that handles URI parsing, API resolution,
/// and content extraction in one call. Returns the runbook ID and content.
pub async fn load_runbook_from_uri(
    client: &HubClient,
    uri: &str,
    display_id: &str,
) -> Result<super::LoadedRunbook, super::RunbookLoadError> {
    use super::RunbookLoadError;
    use uuid::Uuid;

    let parsed = ParsedUri::parse(uri).ok_or_else(|| RunbookLoadError::LoadFailed {
        runbook_id: display_id.to_string(),
        message: format!(
            "Invalid hub URI format: '{}'. Expected 'user/runbook' or 'user/runbook:tag'",
            uri
        ),
    })?;

    // Default to "latest" tag if none specified
    let tag = parsed.tag.as_deref().or(Some("latest"));
    tracing::debug!("Fetching runbook from hub: {} (tag: {:?})", parsed.nwo, tag);

    let (runbook, snapshot) =
        client
            .resolve_by_nwo(&parsed.nwo, tag)
            .await
            .map_err(|e| match e {
                HubError::NotFound(_) => RunbookLoadError::NotFound {
                    runbook_id: display_id.to_string(),
                },
                _ => RunbookLoadError::LoadFailed {
                    runbook_id: display_id.to_string(),
                    message: e.to_string(),
                },
            })?;

    // Parse the runbook ID
    let id = Uuid::parse_str(&runbook.id).map_err(|e| RunbookLoadError::LoadFailed {
        runbook_id: display_id.to_string(),
        message: format!("Invalid runbook ID from hub: {}", e),
    })?;

    // Prefer snapshot content if available, otherwise use runbook content
    let content = if let Some(snapshot) = snapshot {
        tracing::debug!("Using snapshot '{}' content", snapshot.tag);
        snapshot.content
    } else if let Some(content) = runbook.content {
        tracing::debug!("Using runbook content (no snapshot)");
        content
    } else {
        return Err(RunbookLoadError::LoadFailed {
            runbook_id: display_id.to_string(),
            message: "Runbook has no content. You may need to specify a tag.".to_string(),
        });
    };

    Ok(super::LoadedRunbook { id, content })
}

/// Load runbook from hub by ID
///
/// This is a convenience function that handles ID-based lookup and content extraction.
/// Returns the runbook ID and content, or fails if no content is available.
pub async fn load_runbook_from_id(
    client: &HubClient,
    hub_id: &str,
    display_id: &str,
) -> Result<super::LoadedRunbook, super::RunbookLoadError> {
    use super::RunbookLoadError;
    use uuid::Uuid;

    tracing::debug!("Fetching runbook from hub by ID: {}", hub_id);

    let runbook = client
        .get_runbook_by_id(hub_id)
        .await
        .map_err(|e| match e {
            HubError::NotFound(_) => RunbookLoadError::NotFound {
                runbook_id: display_id.to_string(),
            },
            _ => RunbookLoadError::LoadFailed {
                runbook_id: display_id.to_string(),
                message: e.to_string(),
            },
        })?;

    // Parse the runbook ID
    let runbook_uuid = Uuid::parse_str(&runbook.id).map_err(|e| RunbookLoadError::LoadFailed {
        runbook_id: display_id.to_string(),
        message: format!("Invalid runbook ID from hub: {}", e),
    })?;

    // Require content to be present
    let content = runbook
        .content
        .ok_or_else(|| RunbookLoadError::LoadFailed {
            runbook_id: display_id.to_string(),
            message: "Runbook has no content. Specify a tag to load a specific version."
                .to_string(),
        })?;

    Ok(super::LoadedRunbook {
        id: runbook_uuid,
        content,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}
