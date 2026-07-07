use neoism_agent_core::UserModel;
use serde_json::Value;

pub(crate) fn model_from_body(body: &Value) -> Option<UserModel> {
    let mut provider_id = body
        .get("providerID")
        .or_else(|| body.get("providerId"))
        .and_then(Value::as_str)?
        .to_string();
    let model_id = body
        .get("modelID")
        .or_else(|| body.get("modelId"))
        .and_then(Value::as_str)?;
    if provider_id == "neoism" && model_id != "stub" {
        provider_id = std::env::var("NEOISM_AGENT_PROVIDER")
            .unwrap_or_else(|_| "neoism".to_string());
    }
    Some(UserModel {
        provider_id,
        model_id: model_id.to_string(),
        variant: None,
    })
}

pub(crate) fn user_model_from_model_ref(
    model: &neoism_agent_core::ModelRef,
) -> UserModel {
    UserModel {
        provider_id: model.provider_id.clone(),
        model_id: model.id.clone(),
        variant: model.variant.clone(),
    }
}

pub(crate) fn model_ref_from_user_model(
    model: &UserModel,
) -> neoism_agent_core::ModelRef {
    neoism_agent_core::ModelRef {
        provider_id: model.provider_id.clone(),
        id: model.model_id.clone(),
        variant: model.variant.clone(),
    }
}

pub(crate) fn model_ref_from_config(value: &str) -> Option<neoism_agent_core::ModelRef> {
    model_ref_from_config_with_variant(value, None)
}

pub(crate) fn model_ref_from_config_with_variant(
    value: &str,
    variant: Option<String>,
) -> Option<neoism_agent_core::ModelRef> {
    let (provider_id, model_id) = value.split_once('/')?;
    Some(neoism_agent_core::ModelRef {
        provider_id: provider_id.to_string(),
        id: model_id.to_string(),
        variant: variant.filter(|value| !value.trim().is_empty()),
    })
}

pub(crate) fn default_user_model() -> UserModel {
    UserModel {
        provider_id: std::env::var("NEOISM_AGENT_PROVIDER")
            .unwrap_or_else(|_| "neoism".to_string()),
        model_id: std::env::var("NEOISM_AGENT_MODEL")
            .unwrap_or_else(|_| "stub".to_string()),
        variant: None,
    }
}

pub(crate) fn merge_agent_system(
    agent_prompt: Option<&str>,
    request_system: Option<String>,
) -> Option<String> {
    match (
        agent_prompt.filter(|prompt| !prompt.trim().is_empty()),
        request_system.filter(|system| !system.trim().is_empty()),
    ) {
        (Some(prompt), Some(system)) => Some(format!("{prompt}\n\n{system}")),
        (Some(prompt), None) => Some(prompt.to_string()),
        (None, Some(system)) => Some(system),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_model_ref_preserves_reasoning_variant() {
        let model = model_ref_from_config_with_variant(
            "openai/gpt-5.5",
            Some("xhigh".to_string()),
        )
        .expect("model ref should parse");

        assert_eq!(model.provider_id, "openai");
        assert_eq!(model.id, "gpt-5.5");
        assert_eq!(model.variant.as_deref(), Some("xhigh"));
    }
}
