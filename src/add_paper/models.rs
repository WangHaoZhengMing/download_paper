use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 凭证响应
#[derive(Debug, Deserialize)]
pub struct CredentialResponse {
    pub success: bool,
    pub data: Option<CredentialData>,
    pub message: Option<String>,
}

/// 凭证数据
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

/// 临时凭证
#[derive(Debug, Deserialize)]
pub struct Credentials {
    #[serde(rename = "tmpSecretId")]
    pub tmp_secret_id: String,
    #[serde(rename = "tmpSecretKey")]
    pub tmp_secret_key: String,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
}

/// 通知响应
#[derive(Debug, Serialize, Deserialize)]
pub struct NotifyResponse {
    pub success: bool,
    pub data: Option<Value>,
    pub message: Option<String>,
}

/// 保存试卷响应
#[derive(Debug, Serialize, Deserialize)]
pub struct SavePaperResponse {
    pub success: bool,
    pub data: Option<String>,
    pub message: Option<String>,
}

/// 文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub url: String,
    pub key: String,
}

/// LLM 返回的元数据
#[derive(Debug, Clone, Deserialize)]
pub struct MiscByAi {
    pub paper_type_name: String,
    pub school_year_begin: i32,
    pub school_year_end: i32,
    pub paper_term: Option<String>,
    pub paper_month: Option<i16>,
    pub parent_paper_type: String,
}

