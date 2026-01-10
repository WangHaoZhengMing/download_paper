// ============================================================================
// API 响应结构体
// ============================================================================

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct CredentialResponse {
    pub success: bool,
    pub data: Option<CredentialData>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CredentialData {
    pub credentials: Credentials,
    pub region: String,
    pub bucket: String,
    #[serde(rename = "keyPrefix")]
    pub key_prefix: String,
    #[serde(rename = "cdnDomain")]
    pub cdn_domain: String,
}

#[derive(Debug, Deserialize)]
pub struct Credentials {
    #[serde(rename = "tmpSecretId")]
    pub tmp_secret_id: String,
    #[serde(rename = "tmpSecretKey")]
    pub tmp_secret_key: String,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotifyResponse {
    pub success: bool,
    pub data: Option<Value>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavePaperResponse {
    pub success: bool,
    pub data: Option<String>,
    pub message: Option<String>,
}

// ============================================================================
// 文件信息结构体
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub url: String,
    pub key: String,
}
