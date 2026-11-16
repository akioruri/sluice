use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Provider {
    pub name: String,
    pub upstream: String,
    pub auth_header: String,
    pub auth_format: String,
    pub api_key: String,
}

impl Provider {
    pub fn build_auth_header(&self) -> Result<String> {
        Ok(self.auth_format.replace("{key}", &self.api_key))
    }
}
