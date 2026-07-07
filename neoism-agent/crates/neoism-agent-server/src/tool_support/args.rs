use serde_json::Value;

pub(super) fn required_string<'a>(
    arguments: &'a Value,
    key: &str,
) -> anyhow::Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("tool argument {key} is required"))
}

pub(super) fn required_string_either<'a>(
    arguments: &'a Value,
    primary: &str,
    alternate: &str,
) -> anyhow::Result<&'a str> {
    string_either(arguments, primary, alternate)
        .ok_or_else(|| anyhow::anyhow!("tool argument {primary} is required"))
}

pub(super) fn string_either<'a>(
    arguments: &'a Value,
    primary: &str,
    alternate: &str,
) -> Option<&'a str> {
    arguments
        .get(primary)
        .or_else(|| arguments.get(alternate))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn string_either_many<'a>(
    arguments: &'a Value,
    keys: &[&str],
) -> Option<&'a str> {
    for key in keys {
        if let Some(s) = arguments
            .get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            return Some(s);
        }
    }
    None
}

pub(super) fn patch_text_arg(arguments: &Value) -> Option<&str> {
    if let Some(raw) = arguments.as_str().filter(|value| !value.trim().is_empty()) {
        return Some(raw);
    }
    string_either_many(arguments, &["patchText", "patch", "diff", "content"])
}

pub(super) fn required_string_either_many<'a>(
    arguments: &'a Value,
    keys: &[&str],
) -> anyhow::Result<&'a str> {
    string_either_many(arguments, keys).ok_or_else(|| {
        let primary = keys.first().copied().unwrap_or("argument");
        anyhow::anyhow!("tool argument {primary} is required")
    })
}

pub(super) fn optional_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

pub(super) fn usize_arg(arguments: &Value, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}
