use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use super::FromDocument;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum HttpVerb {
    #[default]
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "PUT")]
    Put,
    #[serde(rename = "DELETE")]
    Delete,
    #[serde(rename = "PATCH")]
    Patch,
    #[serde(rename = "HEAD")]
    Head,
}

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Http {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub url: String,

    #[builder(default)]
    pub verb: HttpVerb,

    #[builder(default)]
    pub headers: HashMap<String, String>,
}

impl FromDocument for Http {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Block has no id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Block has no props")?;

        let id = Uuid::parse_str(block_id).map_err(|e| e.to_string())?;

        let verb = props
            .get("verb")
            .and_then(|v| v.as_str())
            .map(|s| match s.to_uppercase().as_str() {
                "GET" => HttpVerb::Get,
                "POST" => HttpVerb::Post,
                "PUT" => HttpVerb::Put,
                "DELETE" => HttpVerb::Delete,
                "PATCH" => HttpVerb::Patch,
                "HEAD" => HttpVerb::Head,
                _ => HttpVerb::Get,
            })
            .unwrap_or_default();

        let headers = props
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let http = Http::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("HTTP Request")
                    .to_string(),
            )
            .url(
                props
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .verb(verb)
            .headers(headers)
            .build();

        Ok(http)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub duration: f64,
    // Note: We're not storing the response body data to avoid storing potentially large payloads
}

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct HttpOutput {
    pub response: HttpResponse,
}
