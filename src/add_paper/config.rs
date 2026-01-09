/// 试卷服务配置
#[derive(Debug, Clone)]
pub struct PaperServiceConfig {
    pub api_base_url: String,
    pub credential_api_path: String,
    pub notify_api_path: String,
    pub save_paper_api_path: String,
    pub tiku_token: String,
    pub js_timeout_secs: u64,
    pub pdf_dir: String,
    pub output_dir: String,
}

impl Default for PaperServiceConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://tps-tiku-api.staff.xdf.cn".to_string(),
            credential_api_path: "/attachment/get/credential".to_string(),
            notify_api_path: "/attachment/batch/upload/files".to_string(),
            save_paper_api_path: "/paper/new/save".to_string(),
            tiku_token: "732FD8402F95087CD934374135C46EE5".to_string(),
            js_timeout_secs: 16,
            pdf_dir: "PDF".to_string(),
            output_dir: "./output_toml".to_string(),
        }
    }
}

