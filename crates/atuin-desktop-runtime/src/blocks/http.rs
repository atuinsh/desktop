use chrono::{DateTime, Utc};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Instant};
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::context::BlockExecutionOutput;
use crate::execution::{ExecutionContext, ExecutionHandle, StreamingBlockOutput};

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Template evaluation error: {0}")]
    Template(#[from] minijinja::Error),
    #[error("HTTP request failed: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
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

impl HttpVerb {
    pub fn is_body_allowed(&self) -> bool {
        !matches!(self, HttpVerb::Get | HttpVerb::Head)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct HttpExecutionOutput {
    pub status: u16,
    pub status_text: String,
    pub status_success: bool,
    pub headers: HashMap<String, String>,
    pub duration_seconds: f64,
    pub body: String,
    pub body_json: Option<serde_json::Value>,
}

impl BlockExecutionOutput for HttpExecutionOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            "status" => Some(minijinja::Value::from(self.status)),
            "status_text" => Some(minijinja::Value::from(self.status_text.clone())),
            "status_success" => Some(minijinja::Value::from(self.status_success)),
            "headers" => Some(minijinja::Value::from_serialize(&self.headers)),
            "duration_seconds" => Some(minijinja::Value::from(self.duration_seconds)),
            "body" => Some(minijinja::Value::from(self.body.clone())),
            "body_json" => Some(minijinja::Value::from_serialize(&self.body_json)),
            _ => None,
        }
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&[
            "status",
            "status_text",
            "status_success",
            "headers",
            "duration_seconds",
            "body",
            "body_json",
        ])
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
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

    #[builder(default)]
    pub body: String,
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
            .and_then(|v| {
                // Support both string format (from frontend) and object format (for backward compatibility)
                if let Some(s) = v.as_str() {
                    // Parse JSON string: "{\"key\":\"value\"}"
                    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(s).ok()
                } else {
                    // Direct object format
                    v.as_object().cloned()
                }
            })
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
            .body(
                props
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            )
            .build();

        Ok(http)
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Http {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Http(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        tracing::trace!("Executing HTTP block {id}", id = self.id);

        let _ = context.block_started().await;

        let block_id = self.id;
        let response = self.make_http_request(&context).await;

        if let Err(e) = response {
            tracing::error!("{e}");

            let error_message = match e {
                HttpError::Reqwest(ref e) => {
                    if e.is_builder() {
                        "Invalid HTTP request".to_string()
                    } else {
                        e.to_string()
                    }
                }
                HttpError::Template(ref e) => e.to_string(),
                HttpError::Other(ref e) => e.to_string(),
            };

            let _ = context.block_failed(error_message).await;
            return Err(e.into());
        }

        let response = response.unwrap();
        let was_success = response.status_success;

        let body_json = serde_json::from_str(&response.body).ok();
        let output = HttpExecutionOutput {
            status: response.status,
            status_text: response.status_text.clone(),
            status_success: response.status_success,
            headers: response.headers.clone(),
            duration_seconds: response.duration,
            body: response.body.clone(),
            body_json,
        };

        let _ = context.set_block_output(output).await;

        let _ = context
            .send_output(
                StreamingBlockOutput::builder()
                    .block_id(block_id)
                    .object(serde_json::to_value(response).map_err(|e| HttpError::Other(e.into()))?)
                    .build(),
            )
            .await;

        let _ = context.block_finished(None, was_success).await;

        Ok(Some(context.handle()))
    }
}

impl Http {
    async fn make_http_request(
        self,
        context: &ExecutionContext,
    ) -> Result<HttpResponse, HttpError> {
        let resolve = |template: &str| -> Result<String, minijinja::Error> {
            context.context_resolver.resolve_template(template)
        };

        let client = Client::new();
        let mut request = client.request(self.verb.clone().into(), resolve(&self.url)?);
        for (key, value) in self.headers {
            request = request.header(resolve(&key)?, resolve(&value)?);
        }
        if !self.body.is_empty() && self.verb.is_body_allowed() {
            request = request.body(resolve(&self.body)?);
        }

        let start_time = Utc::now();
        let start = Instant::now();
        let response = request.send().await?;
        let duration = start.elapsed().as_secs_f64();

        let mut headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(value) = value.to_str() {
                headers.insert(key.to_string(), value.to_string());
            }
        }
        let status = response.status().into();
        let status_success = response.status().is_success();
        let status_text = response.status().to_string();
        let body = response.text().await?;

        Ok(HttpResponse {
            status,
            status_text,
            status_success,
            headers,
            duration,
            body,
            time: start_time,
        })
    }
}

#[derive(TS, Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub status_success: bool,
    pub headers: HashMap<String, String>,
    pub duration: f64,
    #[ts(type = "string")]
    pub time: DateTime<Utc>,
    pub body: String,
}

impl From<HttpVerb> for Method {
    fn from(verb: HttpVerb) -> Self {
        match verb {
            HttpVerb::Get => Method::GET,
            HttpVerb::Post => Method::POST,
            HttpVerb::Put => Method::PUT,
            HttpVerb::Delete => Method::DELETE,
            HttpVerb::Patch => Method::PATCH,
            HttpVerb::Head => Method::HEAD,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;
    use crate::{
        client::{DocumentBridgeMessage, MessageChannel},
        context::ContextResolver,
        document::{actor::DocumentCommand, DocumentHandle},
        events::MemoryEventBus,
        execution::BlockLifecycleEvent,
    };
    use async_trait::async_trait;
    use httpmock::prelude::*;
    use httpmock::Method::HEAD;
    use tokio::sync::{mpsc, Mutex as TokioMutex};

    #[derive(Clone)]
    struct TestMessageChannel {
        messages: Arc<TokioMutex<Vec<DocumentBridgeMessage>>>,
    }

    impl TestMessageChannel {
        fn new() -> Self {
            Self {
                messages: Arc::new(TokioMutex::new(Vec::new())),
            }
        }

        async fn get_messages(&self) -> Vec<DocumentBridgeMessage> {
            self.messages.lock().await.clone()
        }
    }

    #[async_trait]
    impl MessageChannel<DocumentBridgeMessage> for TestMessageChannel {
        async fn send(
            &self,
            message: DocumentBridgeMessage,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.messages.lock().await.push(message);
            Ok(())
        }
    }

    fn create_test_http(url: &str, verb: HttpVerb) -> Http {
        Http::builder()
            .id(Uuid::new_v4())
            .name("Test HTTP")
            .url(url)
            .verb(verb)
            .headers(HashMap::new())
            .body(String::new())
            .build()
    }

    fn create_test_context(block_id: Uuid) -> (ExecutionContext, TestMessageChannel) {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle = DocumentHandle::from_raw(
            "test-runbook".to_string(),
            tx,
            Arc::new(MemoryEventBus::new()),
        );
        let context_resolver = ContextResolver::new();
        let message_channel = TestMessageChannel::new();

        let context = ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .output_channel(Arc::new(message_channel.clone()))
            .handle(ExecutionHandle::new(block_id))
            .build();

        (context, message_channel)
    }

    fn create_test_context_with_vars(
        vars: Vec<(&str, &str)>,
    ) -> (ExecutionContext, TestMessageChannel) {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle = DocumentHandle::from_raw(
            "test-runbook".to_string(),
            tx,
            Arc::new(MemoryEventBus::new()),
        );

        let vars_map: HashMap<String, String> = vars
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let context_resolver = ContextResolver::with_vars(vars_map);

        let message_channel = TestMessageChannel::new();

        let block_id = Uuid::new_v4();
        let context = ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .output_channel(Arc::new(message_channel.clone()))
            .handle(ExecutionHandle::new(block_id))
            .build();

        (context, message_channel)
    }

    fn create_test_context_with_event_bus(
        block_id: Uuid,
        event_bus: Arc<MemoryEventBus>,
    ) -> (ExecutionContext, TestMessageChannel) {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle =
            DocumentHandle::from_raw("test-runbook".to_string(), tx, event_bus.clone());
        let context_resolver = ContextResolver::new();
        let message_channel = TestMessageChannel::new();

        let context = ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .gc_event_bus(event_bus)
            .output_channel(Arc::new(message_channel.clone()))
            .handle(ExecutionHandle::new(block_id))
            .build();

        (context, message_channel)
    }

    #[tokio::test]
    async fn test_successful_get_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/test");
            then.status(200).body("Hello, World!");
        });

        let http = create_test_http(&server.url("/test"), HttpVerb::Get);
        let http_id = http.id;
        let (context, message_channel) = create_test_context(http_id);

        let _ = http.execute(context).await;

        // Wait a bit for the request to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        mock.assert();

        // Verify we received lifecycle messages
        let messages = message_channel.get_messages().await;
        assert_eq!(
            messages.len(),
            3,
            "Expected 3 messages (Started, Output, Finished)"
        );

        // Check Started message
        match &messages[0] {
            DocumentBridgeMessage::BlockOutput { block_id, output } => {
                assert_eq!(*block_id, http_id);
                assert!(matches!(
                    output.lifecycle,
                    Some(BlockLifecycleEvent::Started(_))
                ));
            }
            _ => panic!("Expected BlockOutput message"),
        }

        // Check Output message
        match &messages[1] {
            DocumentBridgeMessage::BlockOutput { block_id, output } => {
                assert_eq!(*block_id, http_id);
                assert!(output.object.is_some(), "Expected response object");
                let response = output.object.as_ref().unwrap();
                assert_eq!(response["status"], 200);
                assert_eq!(response["body"], "Hello, World!");
            }
            _ => panic!("Expected BlockOutput message"),
        }

        // Check Finished message
        match &messages[2] {
            DocumentBridgeMessage::BlockOutput { block_id, output } => {
                assert_eq!(*block_id, http_id);
                match &output.lifecycle {
                    Some(BlockLifecycleEvent::Finished(data)) => {
                        assert!(data.success, "Request should have succeeded");
                    }
                    _ => panic!("Expected Finished lifecycle event"),
                }
            }
            _ => panic!("Expected BlockOutput message"),
        }
    }

    #[tokio::test]
    async fn test_post_request_with_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/data")
                .body("{\"key\":\"value\"}");
            then.status(201)
                .json_body(serde_json::json!({"status": "created"}));
        });

        let mut http = create_test_http(&server.url("/api/data"), HttpVerb::Post);
        let http_id = http.id;
        http.body = "{\"key\":\"value\"}".to_string();

        let (context, message_channel) = create_test_context(http.id());
        let _ = http.execute(context).await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        mock.assert();

        // Verify we received messages
        let messages = message_channel.get_messages().await;
        assert_eq!(
            messages.len(),
            3,
            "Expected 3 messages (Started, Output, Finished)"
        );

        // Check the finished message includes the JSON response
        match &messages[1] {
            DocumentBridgeMessage::BlockOutput { block_id, output } => {
                println!("output: {:?}", output);
                assert_eq!(*block_id, http_id);
                let response = output.object.as_ref().unwrap();
                assert_eq!(response["status"], 201);
                assert_eq!(response["body"], "{\"status\":\"created\"}");
            }
            _ => panic!("Expected BlockOutput message"),
        }
    }

    #[tokio::test]
    async fn test_request_with_headers() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/secure")
                .header("Authorization", "Bearer token123")
                .header("X-Custom-Header", "custom-value");
            then.status(200).body("Authenticated");
        });

        let mut http = create_test_http(&server.url("/secure"), HttpVerb::Get);
        http.headers
            .insert("Authorization".to_string(), "Bearer token123".to_string());
        http.headers
            .insert("X-Custom-Header".to_string(), "custom-value".to_string());

        let (context, _) = create_test_context(http.id());
        let _ = http.execute(context).await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_404_not_found() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/notfound");
            then.status(404).body("Not Found");
        });

        let http = create_test_http(&server.url("/notfound"), HttpVerb::Get);
        let (context, _) = create_test_context(http.id());

        let _ = http.execute(context).await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_500_server_error() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/error");
            then.status(500).body("Internal Server Error");
        });

        let http = create_test_http(&server.url("/error"), HttpVerb::Get);
        let (context, _) = create_test_context(http.id());

        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_variable_substitution_in_url() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/users/123");
            then.status(200)
                .json_body(serde_json::json!({"id": 123, "name": "John"}));
        });

        let vars = vec![("user_id", "123")];
        let (context, _) = create_test_context_with_vars(vars);

        let http = create_test_http(
            &format!("{}/users/{{{{ var.user_id }}}}", server.base_url()),
            HttpVerb::Get,
        );
        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_variable_substitution_in_headers() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api")
                .header("Authorization", "Bearer secret_token");
            then.status(200).body("Success");
        });

        let vars = vec![("api_token", "secret_token")];
        let (context, _) = create_test_context_with_vars(vars);

        let mut http = create_test_http(&server.url("/api"), HttpVerb::Get);
        http.headers.insert(
            "Authorization".to_string(),
            "Bearer {{ var.api_token }}".to_string(),
        );

        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_variable_substitution_in_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/submit")
                .body("{\"username\":\"john_doe\",\"email\":\"john@example.com\"}");
            then.status(201).body("Created");
        });

        let vars = vec![("username", "john_doe"), ("email", "john@example.com")];
        let (context, _) = create_test_context_with_vars(vars);

        let mut http = create_test_http(&server.url("/submit"), HttpVerb::Post);
        http.body =
            "{\"username\":\"{{ var.username }}\",\"email\":\"{{ var.email }}\"}".to_string();

        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_put_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/users/123")
                .body("{\"name\":\"Jane\"}");
            then.status(200).body("Updated");
        });

        let mut http = create_test_http(&server.url("/users/123"), HttpVerb::Put);
        http.body = "{\"name\":\"Jane\"}".to_string();

        let (context, _) = create_test_context(http.id());
        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_delete_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(DELETE).path("/users/123");
            then.status(204);
        });

        let http = create_test_http(&server.url("/users/123"), HttpVerb::Delete);
        let (context, _) = create_test_context(http.id());

        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_patch_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PATCH)
                .path("/users/123")
                .body("{\"email\":\"newemail@example.com\"}");
            then.status(200).body("Patched");
        });

        let mut http = create_test_http(&server.url("/users/123"), HttpVerb::Patch);
        http.body = "{\"email\":\"newemail@example.com\"}".to_string();

        let (context, _) = create_test_context(http.id());
        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_head_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(HEAD).path("/check");
            then.status(200);
        });

        let http = create_test_http(&server.url("/check"), HttpVerb::Head);
        let (context, _) = create_test_context(http.id());

        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_get_request_ignores_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/test");
            then.status(200).body("OK");
        });

        let mut http = create_test_http(&server.url("/test"), HttpVerb::Get);
        http.body = "This should be ignored".to_string();

        let (context, _) = create_test_context(http.id());
        let _ = http.execute(context).await;

        mock.assert();
    }

    #[tokio::test]
    async fn test_grand_central_events_successful_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/test");
            then.status(200).body("OK");
        });

        let http = create_test_http(&server.url("/test"), HttpVerb::Get);
        let http_id = http.id;

        let event_bus = Arc::new(MemoryEventBus::new());
        let (context, _) = create_test_context_with_event_bus(http_id, event_bus.clone());
        let runbook_id = context.runbook_id;

        let _ = http.execute(context).await;

        mock.assert();

        // Verify events were emitted
        use crate::events::GCEvent;
        let events = event_bus.events();
        assert_eq!(events.len(), 2);

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, http_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFinished event
        match &events[1] {
            GCEvent::BlockFinished {
                block_id,
                runbook_id: rb_id,
                success,
            } => {
                assert_eq!(*block_id, http_id);
                assert_eq!(*rb_id, runbook_id);
                assert_eq!(*success, true);
            }
            _ => panic!("Expected BlockFinished event, got: {:?}", events[1]),
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_failed_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/error");
            then.status(500).body("Server Error");
        });

        let http = create_test_http(&server.url("/error"), HttpVerb::Get);
        let http_id = http.id;

        let event_bus = Arc::new(MemoryEventBus::new());
        let (context, _) = create_test_context_with_event_bus(http_id, event_bus.clone());
        let runbook_id = context.runbook_id;

        let _ = http.execute(context).await;

        mock.assert();

        // Verify events were emitted
        use crate::events::GCEvent;
        let events = event_bus.events();
        assert_eq!(events.len(), 2);

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, http_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFinished event (with success=false for 500 error)
        match &events[1] {
            GCEvent::BlockFinished {
                block_id,
                runbook_id: rb_id,
                success,
            } => {
                assert_eq!(*block_id, http_id);
                assert_eq!(*rb_id, runbook_id);
                assert_eq!(*success, false);
            }
            _ => panic!("Expected BlockFinished event, got: {:?}", events[1]),
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_bad_request() {
        let http = create_test_http(&"httasdfa2085!!!!", HttpVerb::Get);
        let http_id = http.id;

        let event_bus = Arc::new(MemoryEventBus::new());
        let (context, _) = create_test_context_with_event_bus(http_id, event_bus.clone());
        let runbook_id = context.runbook_id;

        let _ = http.execute(context).await;

        // Verify events were emitted
        use crate::events::GCEvent;
        let events = event_bus.events();
        assert_eq!(events.len(), 2);

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, http_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFinished event (with success=false for 500 error)
        match &events[1] {
            GCEvent::BlockFailed {
                block_id,
                runbook_id: rb_id,
                error,
            } => {
                assert_eq!(*block_id, http_id);
                assert_eq!(*rb_id, runbook_id);
                assert_eq!(*error, "Invalid HTTP request");
            }
            _ => panic!("Expected BlockFailed event, got: {:?}", events[1]),
        }
    }

    #[tokio::test]
    async fn test_from_document_parsing() {
        let block_data = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "http",
            "props": {
                "name": "Test Request",
                "url": "https://api.example.com/data",
                "verb": "POST",
                "headers": {
                    "Content-Type": "application/json",
                    "Authorization": "Bearer token"
                },
                "body": "{\"test\": true}"
            }
        });

        let http = Http::from_document(&block_data).unwrap();

        assert_eq!(
            http.id,
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
        );
        assert_eq!(http.name, "Test Request");
        assert_eq!(http.url, "https://api.example.com/data");
        assert_eq!(http.verb, HttpVerb::Post);
        assert_eq!(
            http.headers.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(http.headers.get("Authorization").unwrap(), "Bearer token");
        assert_eq!(http.body, "{\"test\": true}");
    }

    #[tokio::test]
    async fn test_from_document_defaults() {
        let block_data = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "http",
            "props": {
                "url": "https://api.example.com"
            }
        });

        let http = Http::from_document(&block_data).unwrap();

        assert_eq!(http.name, "HTTP Request");
        assert_eq!(http.verb, HttpVerb::Get);
        assert!(http.headers.is_empty());
        assert!(http.body.is_empty());
    }

    #[test]
    fn test_from_document_with_string_headers() {
        // Test that headers stored as JSON string (frontend format) are correctly parsed
        let block_data = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "http",
            "props": {
                "url": "https://api.example.com",
                "headers": "{\"Access-Token\":\"abc123\",\"Content-Type\":\"application/json\"}"
            }
        });

        let http = Http::from_document(&block_data).unwrap();

        assert_eq!(http.headers.get("Access-Token").unwrap(), "abc123");
        assert_eq!(
            http.headers.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(http.headers.len(), 2);
    }

    #[test]
    fn test_http_verb_body_allowed() {
        assert!(!HttpVerb::Get.is_body_allowed());
        assert!(!HttpVerb::Head.is_body_allowed());
        assert!(HttpVerb::Post.is_body_allowed());
        assert!(HttpVerb::Put.is_body_allowed());
        assert!(HttpVerb::Delete.is_body_allowed());
        assert!(HttpVerb::Patch.is_body_allowed());
    }
}
