use crate::add_paper::config::PaperServiceConfig;
use crate::add_paper::models::{
    CredentialResponse, NotifyResponse, SavePaperResponse,
};
use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};
use tracing::{debug, error};

/// API 客户端，负责通过 Page 执行 JavaScript 调用 API
#[derive(Clone)]
pub struct ApiClient {
    page: std::sync::Arc<chromiumoxide::Page>,
    config: PaperServiceConfig,
}

impl ApiClient {
    pub fn new(page: std::sync::Arc<chromiumoxide::Page>, config: PaperServiceConfig) -> Self {
        Self { page, config }
    }

    /// 执行 JavaScript 代码并处理超时
    async fn execute_js_with_timeout<T>(
        &self,
        js_code: String,
        args: String,
        timeout_msg: &str,
    ) -> Result<Value>
    where
        T: for<'de> Deserialize<'de>,
    {
        let eval_future = self.page.evaluate(format!("({})({})", js_code, args));
        let eval_result = timeout(
            Duration::from_secs(self.config.js_timeout_secs),
            eval_future,
        )
        .await
        .map_err(|_| anyhow!("{}", timeout_msg))??;
        eval_result
            .into_value()
            .map_err(|e| anyhow!("Failed to get value from evaluation: {}", e))
    }

    /// 获取上传凭证
    pub async fn get_upload_credentials(&self, filename: &str) -> Result<crate::add_paper::models::CredentialData> {
        tracing::info!("--- 阶段1: 正在请求上传凭证 (Via Page Evaluate)... ---");

        let js_code = self.build_credential_request_js();
        let filename_json = serde_json::to_string(filename)?;
        let response_value = self
            .execute_js_with_timeout::<CredentialResponse>(
                js_code,
                filename_json,
                "等待上传凭证响应超时",
            )
            .await?;

        let response: CredentialResponse = serde_json::from_value(response_value)?;

        if response.success && response.data.is_some() {
            tracing::info!("✅ 凭证获取成功。");
            debug!("凭证数据: {:?}", response.data);
            Ok(response.data.unwrap())
        } else {
            let msg = response
                .message
                .unwrap_or_else(|| "Unknown error".to_string());
            error!("❌ 错误: API响应格式不正确或未成功: {}", msg);
            tracing::warn!("❌ 错误: API响应格式不正确或未成功: {}", msg);
            Err(anyhow!("Failed to get credentials: {}", msg))
        }
    }

    /// 通知应用服务器
    pub async fn notify_application_server(
        &self,
        name_for_cos: &str,
        file_info: &crate::add_paper::models::FileInfo,
    ) -> Result<NotifyResponse> {
        tracing::info!("--- 阶段3: 正在通知应用服务器 (Via Page Evaluate)... ---");

        let js_code = self.build_notify_server_js();
        // 使用 name_for_cos 作为 fileName，并添加 .pdf 扩展名
        let file_name_with_ext = format!("{}.pdf", name_for_cos);
        let data = json!({
            "filename": file_name_with_ext,
            "fileUrl": file_info.url
        });

        let response_value = self
            .execute_js_with_timeout::<NotifyResponse>(
                js_code,
                serde_json::to_string(&data)?,
                "通知应用服务器超时",
            )
            .await?;

        let response: NotifyResponse = serde_json::from_value(response_value).map_err(|e| {
            error!("解析通知响应失败: {}", e);
            anyhow!("解析通知响应失败: {}", e)
        })?;
        tracing::info!("✅ 服务器通知成功，已收到返回数据。");
        debug!("通知响应: {:?}", response);
        Ok(response)
    }

    /// 保存试卷
    pub async fn save_paper(&self, payload: &Value) -> Result<SavePaperResponse> {
        let js_code = self.build_save_paper_js();
        let payload_json = serde_json::to_string(payload)?;
        debug!("发送的payload: {}", payload_json);

        let response_value = self
            .execute_js_with_timeout::<SavePaperResponse>(
                js_code,
                payload_json,
                "保存试卷请求超时",
            )
            .await?;

        let result: SavePaperResponse = serde_json::from_value(response_value).map_err(|e| {
            error!("解析保存试卷响应失败: {}", e);
            anyhow!("解析保存试卷响应失败: {}", e)
        })?;
        tracing::debug!("API响应: {}", serde_json::to_string_pretty(&result)?);
        Ok(result)
    }

    /// 生成获取上传凭证的 JavaScript 代码
    fn build_credential_request_js(&self) -> String {
        format!(
            r#"
        async (filename) => {{
            const payload = {{
                fileName: filename,
                contentType: "application/pdf",
                storageType: "cos",
                securityLevel: 1
            }};
            try {{
                const response = await fetch("{}{}", {{
        method: "POST",
        headers: {{
            "Content-Type": "application/json",
                        "Accept": "application/json, text/plain, */*",
                        "tikutoken": "{}"
        }},
        credentials: "include",
                    body: JSON.stringify(payload)
                }});
                const data = await response.json();
            return data;
            }} catch (err) {{
            console.error(err);
            return {{ error: err.toString() }};
            }}
        }}
        "#,
            self.config.api_base_url,
            self.config.credential_api_path,
            self.config.tiku_token
        )
    }

    /// 生成通知应用服务器的 JavaScript 代码
    fn build_notify_server_js(&self) -> String {
        format!(
            r#"
        async (data) => {{
            const url = "{}{}";
            const payload = {{
                uploadAttachments: [{{
                    fileName: data.filename,
                    fileType: "pdf",
                    fileUrl: data.fileUrl,
                    resourceType: "zbtiku_pc"
                }}],
                fileUploadType: 5,
                fileContentType: 1,
                paperId: ""
            }};
            try {{
                const response = await fetch(url, {{
                    method: "POST",
                    headers: {{
                        "Content-Type": "application/json",
                        "Accept": "application/json, text/plain, */*",
                        "tikutoken": "{}"
                    }},
                    credentials: "include",
                    body: JSON.stringify(payload)
                }});
                const resData = await response.json();
                return resData;
            }} catch (e) {{
                console.error("Fetch error:", e);
                return {{ success: false, message: e.toString() }};
            }}
        }}
        "#,
            self.config.api_base_url,
            self.config.notify_api_path,
            self.config.tiku_token
        )
    }

    /// 生成保存试卷的 JavaScript 代码
    fn build_save_paper_js(&self) -> String {
        format!(
            r#"
        async (payload) => {{
            try {{
                const response = await fetch("{}{}", {{
                    method: "POST",
                    headers: {{
                        "Content-Type": "application/json",
                        "Accept": "application/json, text/plain, */*",
                        "tikutoken": "{}"
                    }},
                    credentials: "include",
                    body: JSON.stringify(payload)
                }});
                const data = await response.json();
                return data;
            }} catch (err) {{
                return {{ error: err.toString() }};
            }}
        }}
        "#,
            self.config.api_base_url,
            self.config.save_paper_api_path,
            self.config.tiku_token
        )
    }
}

