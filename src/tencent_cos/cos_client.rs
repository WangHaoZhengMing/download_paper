use std::collections::HashMap;
use hmac::{Hmac, Mac};
use sha1::{Digest, Sha1};
use chrono::{Utc, Duration};
use std::path::Path;
use anyhow::{Result, anyhow};

type HmacSha1 = Hmac<Sha1>;

/// Config类, 保存用户相关信息
#[derive(Debug, Clone)]
pub struct CosConfig {
    pub appid: Option<String>,
    pub secret_id: Option<String>,
    pub secret_key: Option<String>,
    pub token: Option<String>,
    pub region: Option<String>,
    pub scheme: String,
    pub timeout: Option<u64>,
    pub endpoint: Option<String>,
    pub endpoint_ci: Option<String>,
    pub endpoint_pic: Option<String>,
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub anonymous: bool,
    pub ua: Option<String>,
    pub proxies: Option<HashMap<String, String>>,
    pub domain: Option<String>,
    pub service_domain: Option<String>,
    pub keep_alive: bool,
    pub pool_connections: u32,
    pub pool_maxsize: u32,
    pub allow_redirects: bool,
    pub sign_host: bool,
    pub enable_old_domain: bool,
    pub enable_internal_domain: bool,
    pub sign_params: bool,
    pub auto_switch_domain_on_retry: bool,
    pub verify_ssl: Option<String>,
    pub ssl_cert: Option<String>,
    pub copy_part_threshold_size: u64,
}

impl Default for CosConfig {
    fn default() -> Self {
        Self {
            appid: None,
            secret_id: None,
            secret_key: None,
            token: None,
            region: None,
            scheme: "https".to_string(),
            timeout: None,
            endpoint: None,
            endpoint_ci: None,
            endpoint_pic: None,
            ip: None,
            port: None,
            anonymous: false,
            ua: None,
            proxies: None,
            domain: None,
            service_domain: None,
            keep_alive: true,
            pool_connections: 10,
            pool_maxsize: 10,
            allow_redirects: false,
            sign_host: true,
            enable_old_domain: true,
            enable_internal_domain: true,
            sign_params: true,
            auto_switch_domain_on_retry: false,
            verify_ssl: None,
            ssl_cert: None,
            copy_part_threshold_size: 5 * 1024 * 1024 * 1024, // 5GB
        }
    }
}

impl CosConfig {
    /// 创建一个用于临时凭证的简化配置
    pub fn with_temp_credentials(
        region: String,
        secret_id: String,
        secret_key: String,
        token: String,
    ) -> Self {
        let mut config = Self::default();
        config.region = Some(region);
        config.secret_id = Some(secret_id);
        config.secret_key = Some(secret_key);
        config.token = Some(token);
        config.scheme = "https".to_string();
        config
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        appid: Option<String>,
        region: Option<String>,
        secret_id: Option<String>,
        secret_key: Option<String>,
        token: Option<String>,
        scheme: Option<String>,
        timeout: Option<u64>,
        endpoint: Option<String>,
        ip: Option<String>,
        port: Option<u16>,
        anonymous: Option<bool>,
        ua: Option<String>,
        proxies: Option<HashMap<String, String>>,
        domain: Option<String>,
        service_domain: Option<String>,
        keep_alive: Option<bool>,
        pool_connections: Option<u32>,
        pool_maxsize: Option<u32>,
        allow_redirects: Option<bool>,
        sign_host: Option<bool>,
        endpoint_ci: Option<String>,
        endpoint_pic: Option<String>,
        enable_old_domain: Option<bool>,
        enable_internal_domain: Option<bool>,
        sign_params: Option<bool>,
        auto_switch_domain_on_retry: Option<bool>,
        verify_ssl: Option<String>,
        ssl_cert: Option<String>,
    ) -> Self {
        let mut config = Self::default();
        if let Some(v) = appid { config.appid = Some(v); }
        if let Some(v) = region { config.region = Some(v); }
        if let Some(v) = secret_id { config.secret_id = Some(v); }
        if let Some(v) = secret_key { config.secret_key = Some(v); }
        if let Some(v) = token { config.token = Some(v); }
        if let Some(v) = scheme { config.scheme = v; }
        if let Some(v) = timeout { config.timeout = Some(v); }
        if let Some(v) = endpoint { config.endpoint = Some(v); }
        if let Some(v) = ip { config.ip = Some(v); }
        if let Some(v) = port { config.port = Some(v); }
        if let Some(v) = anonymous { config.anonymous = v; }
        if let Some(v) = ua { config.ua = Some(v); }
        if let Some(v) = proxies { config.proxies = Some(v); }
        if let Some(v) = domain { config.domain = Some(v); }
        if let Some(v) = service_domain { config.service_domain = Some(v); }
        if let Some(v) = keep_alive { config.keep_alive = v; }
        if let Some(v) = pool_connections { config.pool_connections = v; }
        if let Some(v) = pool_maxsize { config.pool_maxsize = v; }
        if let Some(v) = allow_redirects { config.allow_redirects = v; }
        if let Some(v) = sign_host { config.sign_host = v; }
        if let Some(v) = endpoint_ci { config.endpoint_ci = Some(v); }
        if let Some(v) = endpoint_pic { config.endpoint_pic = Some(v); }
        if let Some(v) = enable_old_domain { config.enable_old_domain = v; }
        if let Some(v) = enable_internal_domain { config.enable_internal_domain = v; }
        if let Some(v) = sign_params { config.sign_params = v; }
        if let Some(v) = auto_switch_domain_on_retry { config.auto_switch_domain_on_retry = v; }
        if let Some(v) = verify_ssl { config.verify_ssl = Some(v); }
        if let Some(v) = ssl_cert { config.ssl_cert = Some(v); }
        config
    }
}

use std::sync::OnceLock;

static BUILT_IN_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// cos客户端类, 封装相应请求
pub struct CosS3Client {
    pub conf: CosConfig,
    pub retry: u32,
    pub retry_exe_times: u32,
    pub client: reqwest::Client,
    pub use_built_in_pool: bool,
}

impl CosS3Client {
    pub fn new(conf: CosConfig, retry: Option<u32>, client: Option<reqwest::Client>) -> Self {
        let retry = retry.unwrap_or(3);
        let mut use_built_in_pool = false;

        let client = match client {
            Some(c) => c,
            None => {
                use_built_in_pool = true;
                BUILT_IN_CLIENT
                    .get_or_init(|| {
                        reqwest::Client::builder()
                            .pool_max_idle_per_host(conf.pool_connections as usize)
                            .danger_accept_invalid_certs(!conf.verify_ssl.as_deref().map(|v| v == "true").unwrap_or(true))
                            .build()
                            .expect("Failed to create default COS client")
                    })
                    .clone()
            }
        };

        Self {
            conf,
            retry,
            retry_exe_times: 0,
            client,
            use_built_in_pool,
        }
    }

    pub async fn upload_file(&self, bucket: &str, local_file_path: &Path, key: &str) -> Result<()> {
        let file_content = std::fs::read(local_file_path)?;
        let region = self.conf.region.as_deref().ok_or_else(|| anyhow!("Region is required"))?;
        let host = format!("{}.cos.{}.myqcloud.com", bucket, region);
        let url = format!("{}://{}/{}", self.conf.scheme, host, key);

        let method = "PUT";
        let path = format!("/{}", key);
        
        let now = Utc::now();
        let expired = now + Duration::hours(1);
        let key_time = format!("{};{}", now.timestamp(), expired.timestamp());
        
        let secret_id = self.conf.secret_id.as_deref().ok_or_else(|| anyhow!("SecretId is required"))?;
        let secret_key = self.conf.secret_key.as_deref().ok_or_else(|| anyhow!("SecretKey is required"))?;
        
        // 1. SignKey
        let mut mac = HmacSha1::new_from_slice(secret_key.as_bytes()).map_err(|e| anyhow!("{}", e))?;
        mac.update(key_time.as_bytes());
        let sign_key = hex::encode(mac.finalize().into_bytes());
        
        // 2. HttpString
        let http_string = format!("{}\n{}\n\nhost={}\n", method.to_lowercase(), path, host);
        let sha1_http = hex::encode(Sha1::digest(http_string.as_bytes()));
        
        // 3. StringToSign
        let string_to_sign = format!("sha1\n{}\n{}\n", key_time, sha1_http);
        
        // 4. Signature
        let mut mac = HmacSha1::new_from_slice(sign_key.as_bytes()).map_err(|e| anyhow!("{}", e))?;
        mac.update(string_to_sign.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        
        let auth = format!(
            "q-sign-algorithm=sha1&q-ak={}&q-sign-time={}&q-key-time={}&q-header-list=host&q-url-param-list=&q-signature={}",
            secret_id, key_time, key_time, signature
        );

        let mut request = self.client.put(&url)
            .header("Host", &host)
            .header("Authorization", auth);
            
        if let Some(token) = &self.conf.token {
            request = request.header("x-cos-security-token", token);
        }
        
        let response = request.body(file_content).send().await?;
        
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(anyhow!("Upload failed with status {}: {}", status, text))
        }
    }
}
