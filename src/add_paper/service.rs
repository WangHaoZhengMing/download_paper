use crate::add_paper::api_client::ApiClient;
use crate::add_paper::config::PaperServiceConfig;
use crate::add_paper::metadata::MetadataBuilder;
use crate::add_paper::upload::UploadService;
use crate::add_paper::utils::sanitize_filename;
use crate::model::QuestionPage;
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[allow(dead_code)]
/// 试卷服务主结构体
pub struct PaperService {
    api_client: ApiClient,
    upload_service: UploadService,
    config: PaperServiceConfig,
}

// #[allow(dead_code)]
impl PaperService {
    /// 创建新的 PaperService 实例
    pub fn new(page: Arc<chromiumoxide::Page>, config: Option<PaperServiceConfig>) -> Self {
        let config = config.unwrap_or_default();
        let api_client = ApiClient::new(page, config.clone());
        let upload_service = UploadService::new(api_client.clone());
        Self {
            api_client,
            upload_service,
            config,
        }
    }

    /// 保存新试卷
    pub async fn save_new_paper(&self, question_page: &mut QuestionPage) -> Result<Option<String>> {
        // 获取用于 COS 上传的文件名
        let name_for_cos = question_page.get_name_for_cos();
        // 使用 name_for_cos 生成 PDF 路径（因为 PDF 文件是用这个名称保存的）
        let pdf_path = format!("{}/{}.pdf", self.config.pdf_dir, name_for_cos);
        let attachments = self
            .upload_service
            .upload_pdf(Path::new(&pdf_path), &question_page.get_name_for_cos())
            .await?;
        debug!("attachments are:{:?}", &attachments);

        // 构建保存试卷的 payload
        let payload = MetadataBuilder::build_paper_payload(question_page, attachments).await?;
        let payload_json = serde_json::to_string(&payload)?;
        debug!("发送的payload: {}", payload_json);
        debug!(
            "Payload 详细内容: {}",
            serde_json::to_string_pretty(&payload)?
        );

        // 调用保存试卷 API
        let result = self.api_client.save_paper(&payload).await?;

        if result.success {
            if let Some(paper_id) = result.data {
                info!("✅ 成功! 获取到的paper_id: {}", paper_id);
                debug!("试卷保存成功，paper_id: {}", paper_id);
                question_page.page_id = Some(paper_id.clone());
                self.save_paper_to_toml(question_page).map_err(|e| {
                    error!("保存 TOML 文件失败: {:?}", e);
                    e
                })?;
                info!("TOML 文件保存成功");
                Ok(Some(paper_id))
            } else {
                error!("❌ API 返回成功但未包含 paper_id");
                warn!("❌ API 返回成功但未包含 paper_id");
                Ok(None)
            }
        } else {
            let msg = result.message.as_deref().unwrap_or("Unknown error");
            error!("❌ save failed: {}", msg);
            error!("详细响应: {:?}", result);
            Ok(None)
        }
    }

    /// 保存试卷到 TOML 文件
    fn save_paper_to_toml(&self, question_page: &QuestionPage) -> Result<()> {
        let output_dir = Path::new(&self.config.output_dir);
        fs::create_dir_all(output_dir)?;
        // 使用清理后的文件名，确保文件系统兼容性
        let sanitized_name = sanitize_filename(&question_page.name);
        let toml_path = output_dir.join(format!("{}.toml", sanitized_name));
        let toml_content = toml::to_string(question_page)?;
        fs::write(toml_path, toml_content)?;
        Ok(())
    }
}

