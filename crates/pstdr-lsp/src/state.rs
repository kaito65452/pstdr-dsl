use std::collections::HashMap;
use lsp_types::Uri;
use pstdr_lang::spec::BedrockSpec;

pub struct ServerState {
    pub docs: HashMap<Uri, String>,
    pub spec: BedrockSpec,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            spec: BedrockSpec::bundled(),
        }
    }

    pub fn uri_to_filename(uri: &Uri) -> String {
        uri.as_str()
            .strip_prefix("file://")
            .unwrap_or(uri.as_str())
            .to_string()
    }
}

