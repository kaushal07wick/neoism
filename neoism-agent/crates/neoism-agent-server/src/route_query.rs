use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct InstanceQuery {
    pub directory: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VcsDiffQuery {
    pub mode: Option<String>,
}
