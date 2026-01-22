use std::{future::Future, ops::Deref, pin::Pin, sync::Arc};

use genai::{
    adapter::AdapterKind,
    resolver::{AuthData, Endpoint, ServiceTargetResolver},
    ClientConfig, ModelIden, ServiceTarget,
};

use crate::secret_cache::{SecretCache, SecretCacheError};

#[derive(Debug, thiserror::Error)]
pub enum AtuinAIClientError {
    #[error("Failed to get credential: {0}")]
    CredentialError(#[from] SecretCacheError),
}

/// A wrapper around a genai::Client that includes Atuin's custom service target resolver
pub struct AtuinAIClient {
    client: genai::Client,
    #[allow(dead_code)] // this will be used to fetch provider API keys in the future
    secret_cache: Arc<SecretCache>,
}

impl AtuinAIClient {
    pub fn new(secret_cache: Arc<SecretCache>) -> Self {
        let secret_cache_clone = secret_cache.clone();
        let target_resolver =
            ServiceTargetResolver::from_resolver_async_fn(move |service_target| {
                resolve_service_target(service_target, secret_cache_clone.clone())
            });
        let client = genai::Client::builder()
            .with_config(ClientConfig::default().with_service_target_resolver(target_resolver))
            .build();

        Self {
            client,
            secret_cache,
        }
    }
}

impl Deref for AtuinAIClient {
    type Target = genai::Client;

    fn deref(&self) -> &genai::Client {
        &self.client
    }
}

fn resolve_service_target(
    mut service_target: ServiceTarget,
    secret_cache: Arc<SecretCache>,
) -> Pin<Box<dyn Future<Output = Result<ServiceTarget, genai::resolver::Error>> + Send>> {
    Box::pin(async move {
        let model_name = service_target.model.model_name.to_string();
        let parts = model_name.splitn(3, "::").collect::<Vec<&str>>();

        if parts.len() != 3 {
            return Err(genai::resolver::Error::Custom(format!(
                "Invalid Atuin Desktop model identifier format: {}",
                model_name
            )));
        }

        // Set the adapter kind based on the provider
        let adapter_kind = match parts[0] {
            "atuinhub" => AdapterKind::Anthropic,
            "claude" => AdapterKind::Anthropic,
            "openai" => AdapterKind::OpenAI,
            "ollama" => AdapterKind::Ollama,
            _ => {
                return Err(genai::resolver::Error::Custom(format!(
                    "Invalid provider identifier: {}",
                    parts[0]
                )))
            }
        };

        // Set the API key, if any, for the provider
        let key = get_api_key(&secret_cache, adapter_kind, parts[0] == "atuinhub")
            .await
            .map_err(|e| genai::resolver::Error::Custom(e.to_string()))?;

        if let Some(key) = key {
            let auth = AuthData::Key(key);
            service_target.auth = auth;
        } else {
            service_target.auth = AuthData::Key("".to_string());
        }

        // Set the specific model
        let model_id = ModelIden::new(adapter_kind, parts[1]);
        service_target.model = model_id;

        // Set the endpoint, if any, for the model
        if parts[2] != "default" {
            service_target.endpoint = Endpoint::from_owned(parts[2].to_string());
        }

        Ok(service_target)
    })
}

async fn get_api_key(
    _secret_cache: &SecretCache,
    _adapter_kind: AdapterKind,
    is_hub: bool,
) -> Result<Option<String>, AtuinAIClientError> {
    // todo
    if is_hub {
        return Ok(None);
    }

    Ok(None)
}
