use crate::add_paper::api_client::ApiClient;
use crate::add_paper::models::{CredentialData, FileInfo};
use crate::tencent_cos::{CosConfig, CosS3Client};
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::path::Path;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// 文件上传服务
pub struct UploadService {
    api_client: ApiClient,
}

impl UploadService {
    pub fn new(api_client: ApiClient) -> Self {
        Self { api_client }
    }

    /// 上传 PDF 文件并获取附件信息（带重试机制）
    pub async fn upload_pdf(&self, file_path: &Path, name_for_cos: &str) -> Result<Option<Value>> {
        if !file_path.exists() {
            return Err(anyhow!("File '{:?}' does not exist", file_path));
        }

        const MAX_RETRIES: u32 = 3;
        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            info!("📤 尝试上传 PDF (第 {}/{} 次)", attempt, MAX_RETRIES);
            
            match self.try_upload_once(file_path, name_for_cos).await {
                Ok(Some(data)) => {
                    info!("✅ 上传成功！");
                    return Ok(Some(data));
                }
                Ok(None) => {
                    warn!("⚠️ 第 {} 次上传失败：服务器返回错误", attempt);
                    if attempt < MAX_RETRIES {
                        let delay = attempt as u64 * 2; // 递增延迟：2秒、4秒、6秒
                        warn!("⏳ {} 秒后重试...", delay);
                        sleep(tokio::time::Duration::from_secs(delay)).await;
                    } else {
                        last_error = Some(anyhow!("上传失败：已重试 {} 次，服务器均返回错误", MAX_RETRIES));
                    }
                }
                Err(e) => {
                    error!("❌ 第 {} 次上传出错: {}", attempt, e);
                    if attempt < MAX_RETRIES {
                        let delay = attempt as u64 * 2;
                        warn!("⏳ {} 秒后重试...", delay);
                        sleep(tokio::time::Duration::from_secs(delay)).await;
                    } else {
                        last_error = Some(anyhow!("上传失败：已重试 {} 次，最后一次错误: {}", MAX_RETRIES, e));
                    }
                }
            }
        }

        // 所有重试都失败，返回错误
        error!("❌ 上传最终失败，已重试 {} 次", MAX_RETRIES);
        Err(last_error.unwrap_or_else(|| anyhow!("上传失败：未知错误")))
    }

    /// 单次上传尝试
    async fn try_upload_once(&self, file_path: &Path, name_for_cos: &str) -> Result<Option<Value>> {
        // 使用传入的 name_for_cos 作为文件名（用于 COS 上传和通知服务器）
        let credentials = self.api_client.get_upload_credentials(name_for_cos).await?;
        let file_info = self.upload_to_cos(credentials, file_path, name_for_cos).await?;
        let notify_response = self
            .api_client
            .notify_application_server(name_for_cos, &file_info)
            .await?;

        if notify_response.success && notify_response.data.is_some() {
            let data_array = &notify_response.data.unwrap();
            info!("🎉 成功获取到目标 `data` 数组! 🎉");
            debug!("附件数据: {:?}", data_array);
            Ok(Some(data_array.clone()))
        } else {
            error!("上传流程完成但未获取到附件数据,服务器返回内容如下");
            error!("{}", serde_json::to_string_pretty(&notify_response)?);
            Ok(None)
        }
    }

    /// 上传文件到腾讯云COS
    async fn upload_to_cos(
        &self,
        credentials_data: CredentialData,
        file_path: &Path,
        filename: &str,
    ) -> Result<FileInfo> {
        info!("--- 阶段2: 正在上传文件到腾讯云COS... ---");

        let creds = &credentials_data.credentials;
        let config = CosConfig::with_temp_credentials(
            credentials_data.region.clone(),
            creds.tmp_secret_id.clone(),
            creds.tmp_secret_key.clone(),
            creds.session_token.clone(),
        );

        let client = CosS3Client::new(config, None, None);
        let bucket = &credentials_data.bucket;
        // 清理 key_prefix：去除前后斜杠和空格
        let key_prefix = credentials_data
            .key_prefix
            .trim()
            .trim_start_matches('/')
            .trim_end_matches('/');
        // 使用传入的 filename（已经清理过），添加 .pdf 扩展名用于云端存储
        let filename_with_ext = format!("{}.pdf", filename);
        // 生成 object_key，确保格式正确（无前导斜杠）
        let object_key = format!("{}/{}/{}", key_prefix, Uuid::new_v4(), filename_with_ext);

        debug!("原始文件路径: {:?}", file_path);
        debug!("使用的文件名: {:?}", filename);
        debug!("云端路径 (Key): {}", object_key);

        debug!(
            "开始上传文件到 COS，bucket: {}, key: {}",
            bucket, object_key
        );
        client
            .upload_file(bucket, file_path, &object_key)
            .await
            .map_err(|e| {
                error!("文件上传到 COS 失败: {}", e);
                e
            })?;

        let final_url = format!("https://{}/{}", credentials_data.cdn_domain, object_key);
        info!("✅ 文件上传成功。");
        info!("最终文件URL: {}", final_url);
        debug!("文件上传完成，URL: {}", final_url);

        Ok(FileInfo {
            url: final_url,
            key: object_key,
        })
    }
}

