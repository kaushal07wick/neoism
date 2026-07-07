use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFrontmatter {
    pub end_line: usize,
    pub properties: Vec<ParsedProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProperty {
    pub key: String,
    pub value: String,
    pub value_type: String,
}

pub fn parse_frontmatter(source: &str) -> Option<ParsedFrontmatter> {
    let mut lines = source.lines();
    let marker = lines.next()?.trim();
    if !matches!(marker, "---" | "+++") {
        return None;
    }

    let mut body = Vec::new();
    let mut end_line = None;
    for (offset, line) in lines.enumerate() {
        let line_no = offset + 2;
        if line.trim() == marker {
            end_line = Some(line_no);
            break;
        }
        body.push(line);
    }
    let end_line = end_line?;
    let body = body.join("\n");
    let properties = match marker {
        "---" => parse_yaml_properties(&body),
        "+++" => parse_toml_properties(&body),
        _ => Vec::new(),
    };

    Some(ParsedFrontmatter {
        end_line,
        properties,
    })
}

fn parse_yaml_properties(source: &str) -> Vec<ParsedProperty> {
    let Ok(value) = serde_yaml::from_str::<Value>(source) else {
        return Vec::new();
    };
    top_level_properties(value)
}

fn parse_toml_properties(source: &str) -> Vec<ParsedProperty> {
    let Ok(value) = toml::from_str::<toml::Value>(source) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::to_value(value) else {
        return Vec::new();
    };
    top_level_properties(value)
}

fn top_level_properties(value: Value) -> Vec<ParsedProperty> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    let mut properties = object
        .iter()
        .filter_map(|(key, value)| {
            serde_json::to_string(value)
                .ok()
                .map(|encoded| ParsedProperty {
                    key: key.clone(),
                    value: encoded,
                    value_type: value_type(value).to_string(),
                })
        })
        .collect::<Vec<_>>();
    properties.sort_by(|a, b| a.key.cmp(&b.key));
    properties
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_yaml_frontmatter_properties() {
        let parsed = parse_frontmatter(
            "---\ntitle: Roadmap\ntags:\n  - neoism\npriority: 2\n---\n# Body\n",
        )
        .unwrap();

        assert_eq!(parsed.end_line, 6);
        assert!(parsed
            .properties
            .iter()
            .any(|property| property.key == "title" && property.value == "\"Roadmap\""));
        assert!(parsed
            .properties
            .iter()
            .any(|property| property.key == "tags" && property.value_type == "array"));
    }

    #[test]
    fn parses_toml_frontmatter_properties() {
        let parsed =
            parse_frontmatter("+++\ntitle = \"Roadmap\"\ndone = false\n+++\n").unwrap();

        assert_eq!(parsed.end_line, 4);
        assert!(parsed
            .properties
            .iter()
            .any(|property| property.key == "done" && property.value == "false"));
    }
}
