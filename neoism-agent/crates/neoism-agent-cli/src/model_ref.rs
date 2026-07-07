pub(crate) fn split_model_ref(model: &str) -> Option<(String, String)> {
    let (provider, model) = model.split_once('/')?;
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    Some((provider.to_string(), model.to_string()))
}
