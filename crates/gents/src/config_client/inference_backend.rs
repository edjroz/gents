use crate::graphql::escape_graphql_string;
use crate::{BackendProviderKind, OpenAiWireApi};
use anyhow::Result;

use super::{mint_recreate_identity_timestamp, ConfigAccess};
use gents_protocol::graphql::{graphql_bool_literal, nullable_string_field, string_list_field};

#[derive(Debug, Clone)]
pub struct InferenceBackendUpsertDocument {
    pub backend_id: String,
    pub name: String,
    pub provider_kind: BackendProviderKind,
    pub openai_wire_api: Option<OpenAiWireApi>,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub api_key_env_var: Option<String>,
    pub max_concurrent: i64,
    pub max_queue_depth: i64,
    pub enabled: bool,
    pub models_on_add: Vec<String>,
    pub models_on_update: Option<Vec<String>>,
    pub probe_status: String,
}

pub async fn write_inference_backend_document(
    access: &ConfigAccess,
    backend: &InferenceBackendUpsertDocument,
) -> Result<String> {
    // Claude is not an OpenAI-wire provider. When migrating an existing
    // OpenAiCompatible/:8787 backend, force-clear sticky openai_wire_api on
    // update so ChatCompletions cannot linger on the document.
    let clear_update_fields = openai_wire_api_clear_fields(backend);
    write_inference_backend_document_with_clear_fields(access, backend, clear_update_fields).await
}

pub async fn write_inference_backend_document_with_clear_fields(
    access: &ConfigAccess,
    backend: &InferenceBackendUpsertDocument,
    clear_update_fields: &[&str],
) -> Result<String> {
    let mutation = render_inference_backend_upsert_mutation(backend, clear_update_fields)?;
    let response = access
        .execute_mutation(&mutation, "upsert InferenceBackend")
        .await?;
    gents_protocol::graphql::extract_mutation_doc_id(&response, "InferenceBackend")
}

fn openai_wire_api_clear_fields(backend: &InferenceBackendUpsertDocument) -> &'static [&'static str] {
    if backend.provider_kind == BackendProviderKind::ClaudeCliSubscription {
        &["openai_wire_api"]
    } else {
        &[]
    }
}

fn normalized_openai_wire_api(
    backend: &InferenceBackendUpsertDocument,
) -> Option<OpenAiWireApi> {
    if backend.provider_kind == BackendProviderKind::ClaudeCliSubscription {
        None
    } else {
        backend.openai_wire_api
    }
}

fn render_inference_backend_upsert_mutation(
    backend: &InferenceBackendUpsertDocument,
    clear_update_fields: &[&str],
) -> Result<String> {
    let recreate_identity = escape_graphql_string(&mint_recreate_identity_timestamp());
    let models_add = string_list_field("models", &backend.models_on_add)
        .ok_or_else(|| anyhow::anyhow!("backend models field could not be rendered"))?;
    let models_update = backend
        .models_on_update
        .as_ref()
        .and_then(|models| string_list_field("models", models));
    let openai_wire_api = normalized_openai_wire_api(backend);
    let omit_openai_wire_api_update = clear_update_fields
        .iter()
        .any(|field| *field == "openai_wire_api");
    let mut update_fields = vec![
        Some(format!(
            r#"name: "{}""#,
            escape_graphql_string(&backend.name)
        )),
        Some(format!(
            r#"provider_kind: "{}""#,
            escape_graphql_string(backend.provider_kind.as_str())
        )),
        (!omit_openai_wire_api_update).then(|| {
            nullable_string_field(
                "openai_wire_api",
                openai_wire_api.map(OpenAiWireApi::as_str),
            )
        }),
        Some(format!(
            r#"endpoint: "{}""#,
            escape_graphql_string(&backend.endpoint)
        )),
        Some(nullable_string_field("api_key", backend.api_key.as_deref())),
        Some(nullable_string_field(
            "api_key_env_var",
            backend.api_key_env_var.as_deref(),
        )),
        Some(format!("max_concurrent: {}", backend.max_concurrent)),
        Some(format!("max_queue_depth: {}", backend.max_queue_depth)),
        Some(format!(
            "enabled: {}",
            graphql_bool_literal(backend.enabled)
        )),
        models_update,
        Some(format!(
            r#"probe_status: "{}""#,
            escape_graphql_string(&backend.probe_status)
        )),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !clear_update_fields.is_empty() {
        update_fields.extend(
            clear_update_fields
                .iter()
                .map(|field| format!("{field}: null")),
        );
    }
    let update_fields = update_fields.join(",\n                    ");
    Ok(format!(
        r#"mutation {{
            upsert_InferenceBackend(
                filter: {{ backend_id: {{ _eq: "{backend_id}" }} }},
                add: {{
                    backend_id: "{backend_id}",
                    name: "{name}",
                    provider_kind: "{provider_kind}",
                    {openai_wire_api_field},
                    endpoint: "{endpoint}",
                    {api_key},
                    {api_key_env_var},
                    max_concurrent: {max_concurrent},
                    max_queue_depth: {max_queue_depth},
                    enabled: {enabled},
                    {models_add},
                    probe_status: "{probe_status}",
                    updated_at: "{recreate_identity}"
                }},
                update: {{
                    {update_fields}
                }}
            ) {{ _docID }}
        }}"#,
        backend_id = escape_graphql_string(&backend.backend_id),
        name = escape_graphql_string(&backend.name),
        provider_kind = escape_graphql_string(backend.provider_kind.as_str()),
        openai_wire_api_field = nullable_string_field(
            "openai_wire_api",
            openai_wire_api.map(OpenAiWireApi::as_str)
        ),
        endpoint = escape_graphql_string(&backend.endpoint),
        api_key = nullable_string_field("api_key", backend.api_key.as_deref()),
        api_key_env_var =
            nullable_string_field("api_key_env_var", backend.api_key_env_var.as_deref()),
        max_concurrent = backend.max_concurrent,
        max_queue_depth = backend.max_queue_depth,
        enabled = graphql_bool_literal(backend.enabled),
        models_add = models_add,
        probe_status = escape_graphql_string(&backend.probe_status),
        recreate_identity = recreate_identity,
        update_fields = update_fields,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_doc(openai_wire_api: Option<OpenAiWireApi>) -> InferenceBackendUpsertDocument {
        InferenceBackendUpsertDocument {
            backend_id: "claude".to_string(),
            name: "Claude".to_string(),
            provider_kind: BackendProviderKind::ClaudeCliSubscription,
            openai_wire_api,
            endpoint: crate::claude_subscription::DEFAULT_BACKEND_ENDPOINT.to_string(),
            api_key: None,
            api_key_env_var: None,
            max_concurrent: 1,
            max_queue_depth: 100,
            enabled: true,
            models_on_add: vec!["claude-sonnet-5".to_string()],
            models_on_update: None,
            probe_status: "healthy".to_string(),
        }
    }

    #[test]
    fn claude_upsert_clears_sticky_openai_wire_api_on_update() {
        let mutation = render_inference_backend_upsert_mutation(
            &claude_doc(Some(OpenAiWireApi::ChatCompletions)),
            openai_wire_api_clear_fields(&claude_doc(Some(OpenAiWireApi::ChatCompletions))),
        )
        .expect("render mutation");

        assert!(
            mutation.contains(r#"provider_kind: "ClaudeCliSubscription""#),
            "mutation must target ClaudeCliSubscription: {mutation}"
        );
        assert!(
            mutation.contains("openai_wire_api: null"),
            "Claude add/update must null openai_wire_api: {mutation}"
        );
        let update = mutation
            .split("update: {")
            .nth(1)
            .expect("update clause present");
        assert!(
            update.contains("openai_wire_api: null"),
            "update must clear sticky openai_wire_api: {update}"
        );
        assert!(
            !update.contains("chat_completions"),
            "sticky ChatCompletions must not survive Claude migration: {update}"
        );
    }
}
