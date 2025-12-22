use crate::ask_llm::resolve_city_with_llm;
use crate::bank_page_info::address::{get_city_code, match_cities_from_paper_name};
use crate::bank_page_info::grade::find_grade_code;
use crate::bank_page_info::subject::find_subject_code;
use crate::model::QuestionPage;
use crate::tencent_cos::{CosConfig, CosS3Client};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use tokio::time::{Duration, timeout};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ============================================================================
// 常量定义
// ============================================================================

const API_BASE_URL: &str = "https://tps-tiku-api.staff.xdf.cn";
const CREDENTIAL_API_PATH: &str = "/attachment/get/credential";
const NOTIFY_API_PATH: &str = "/attachment/batch/upload/files";
const SAVE_PAPER_API_PATH: &str = "/paper/new/save";
const TIKU_TOKEN: &str = "732FD8402F95087CD934374135C46EE5";
const JS_TIMEOUT_SECS: u64 = 16;
const PDF_DIR: &str = "PDF";
const OUTPUT_DIR: &str = "./output_toml";

// ============================================================================
// API 响应结构体
// ============================================================================

#[derive(Debug, Deserialize)]
struct CredentialResponse {
    success: bool,
    data: Option<CredentialData>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CredentialData {
    credentials: Credentials,
    region: String,
    bucket: String,
    #[serde(rename = "keyPrefix")]
    key_prefix: String,
    #[serde(rename = "cdnDomain")]
    cdn_domain: String,
}

#[derive(Debug, Deserialize)]
struct Credentials {
    #[serde(rename = "tmpSecretId")]
    tmp_secret_id: String,
    #[serde(rename = "tmpSecretKey")]
    tmp_secret_key: String,
    #[serde(rename = "sessionToken")]
    session_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NotifyResponse {
    success: bool,
    data: Option<Value>,
    message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavePaperResponse {
    success: bool,
    data: Option<String>,
    message: Option<String>,
}

// ============================================================================
// 文件信息结构体
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileInfo {
    url: String,
    key: String,
}

// ============================================================================
// JavaScript 代码生成器
// ============================================================================

/// 生成获取上传凭证的 JavaScript 代码
fn build_credential_request_js() -> String {
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
                const response = await fetch("{API_BASE_URL}{CREDENTIAL_API_PATH}", {{
        method: "POST",
        headers: {{
            "Content-Type": "application/json",
                        "Accept": "application/json, text/plain, */*",
                        "tikutoken": "{TIKU_TOKEN}"
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
        "#
    )
}

/// 生成通知应用服务器的 JavaScript 代码
fn build_notify_server_js() -> String {
    format!(
        r#"
        async (data) => {{
            const url = "{API_BASE_URL}{NOTIFY_API_PATH}";
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
                        "tikutoken": "{TIKU_TOKEN}"
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
        "#
    )
}

/// 生成保存试卷的 JavaScript 代码
fn build_save_paper_js() -> String {
    format!(
        r#"
        async (payload) => {{
            try {{
                const response = await fetch("{API_BASE_URL}{SAVE_PAPER_API_PATH}", {{
                    method: "POST",
                    headers: {{
                        "Content-Type": "application/json",
                        "Accept": "application/json, text/plain, */*"
                    }},
                    credentials: "include",
                    body: payload
                }});
                const data = await response.json();
                return data;
            }} catch (err) {{
                return {{ error: err.toString() }};
            }}
        }}
        "#
    )
}

// ============================================================================
// 通用辅助函数
// ============================================================================

/// 执行 JavaScript 代码并处理超时
async fn execute_js_with_timeout<T>(
    page: &chromiumoxide::Page,
    js_code: String,
    args: String,
    timeout_msg: &str,
) -> Result<Value>
where
    T: for<'de> Deserialize<'de>,
{
    // 对于字符串参数，需要确保正确转义
    // 如果args已经是JSON字符串，直接使用；否则需要序列化
    let eval_future = page.evaluate(format!("({})({})", js_code, args));
    let eval_result = timeout(Duration::from_secs(JS_TIMEOUT_SECS), eval_future)
        .await
        .map_err(|_| anyhow!("{}", timeout_msg))??;
    eval_result
        .into_value()
        .map_err(|e| anyhow!("Failed to get value from evaluation: {}", e))
}

/// 从文件路径获取文件名
fn get_filename(file_path: &Path) -> Result<&str> {
    file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Invalid filename"))
}

// ============================================================================
// API 调用函数
// ============================================================================

/// 阶段1: 获取上传凭证
async fn get_upload_credentials(
    page: &chromiumoxide::Page,
    filename: &str,
) -> Result<CredentialData> {
    info!("--- 阶段1: 正在请求上传凭证 (Via Page Evaluate)... ---");

    let js_code = build_credential_request_js();
    let filename_json = serde_json::to_string(filename)?;
    let response_value = execute_js_with_timeout::<CredentialResponse>(
        page,
        js_code,
        filename_json,
        "等待上传凭证响应超时",
    )
    .await?;

    let response: CredentialResponse = serde_json::from_value(response_value)?;

    if response.success && response.data.is_some() {
        info!("✅ 凭证获取成功。");
        Ok(response.data.unwrap())
    } else {
        let msg = response
            .message
            .unwrap_or_else(|| "Unknown error".to_string());
        warn!("❌ 错误: API响应格式不正确或未成功: {}", msg);
        Err(anyhow!("Failed to get credentials: {}", msg))
    }
}

/// 阶段2: 上传文件到腾讯云COS
async fn upload_to_cos(credentials_data: CredentialData, file_path: &Path) -> Result<FileInfo> {
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
    // 清理 filename：去除前后空格（确保没有空格）
    let filename = get_filename(file_path)?.trim();
    // 生成 object_key，确保格式正确（无前导斜杠）
    let object_key = format!("{}/{}/{}", key_prefix, Uuid::new_v4(), filename);

    debug!("原始文件名: {:?}", get_filename(file_path)?);
    debug!("清理后文件名: {:?}", filename);
    debug!("云端路径 (Key): {}", object_key);

    client.upload_file(bucket, file_path, &object_key).await?;

    let final_url = format!("https://{}/{}", credentials_data.cdn_domain, object_key);
    info!("✅ 文件上传成功。");
    info!("最终文件URL: {}", final_url);

    Ok(FileInfo {
        url: final_url,
        key: object_key,
    })
}

/// 阶段3: 通知应用服务器
async fn notify_application_server(
    page: &chromiumoxide::Page,
    filename: &str,
    file_info: &FileInfo,
) -> Result<NotifyResponse> {
    info!("--- 阶段3: 正在通知应用服务器 (Via Page Evaluate)... ---");

    let js_code = build_notify_server_js();
    let data = json!({
        "filename": filename,
        "fileUrl": file_info.url
    });

    let response_value = execute_js_with_timeout::<NotifyResponse>(
        page,
        js_code,
        serde_json::to_string(&data)?,
        "通知应用服务器超时",
    )
    .await?;

    let response: NotifyResponse = serde_json::from_value(response_value)?;
    info!("✅ 服务器通知成功，已收到返回数据。");
    Ok(response)
}

// ============================================================================
// 文件上传流程
// ============================================================================
/// 上传 PDF 文件并获取附件信息
async fn upload_pdf(page: &chromiumoxide::Page, file_path: &Path) -> Result<Option<Value>> {
    if !file_path.exists() {
        return Err(anyhow!("File '{:?}' does not exist", file_path));
    }

    // 清理文件名：去除前后空格
    let filename = get_filename(file_path)?.trim();
    let credentials = get_upload_credentials(page, filename).await?;
    let file_info = upload_to_cos(credentials, file_path).await?;
    let notify_response = notify_application_server(page, filename, &file_info).await?;

    if notify_response.success && notify_response.data.is_some() {
        let data_array = &notify_response.data.unwrap();
        info!("{}", "=".repeat(50));
        info!("🎉 成功获取到目标 `data` 数组! 🎉");
        Ok(Some(data_array.clone()))
    } else {
        warn!("未能从最终响应中找到 'data' 数组。服务器返回内容如下:");
        debug!("{}", serde_json::to_string_pretty(&notify_response)?);
        Ok(None)
    }
}

// ============================================================================
// 试卷保存相关函数
// ============================================================================

/// 从试卷名称中确定城市（先匹配，如果结果不是1个则调用LLM裁决）
async fn determine_city_from_paper_name(paper_name: &str, province: &str) -> Result<Option<i16>> {
    // 1. 先用 Rust 代码匹配城市
    let matched_cities = match_cities_from_paper_name(paper_name, Some(province));

    info!(
        "从试卷名称 '{}' 中匹配到 {} 个城市: {:?}",
        paper_name,
        matched_cities.len(),
        matched_cities
    );

    // 2. 根据匹配结果决定下一步
    let city_name = match matched_cities.len() {
        0 => {
            // 没有匹配到城市
            warn!("未匹配到任何城市");
            None
        }
        1 => {
            // 正好匹配到1个，直接使用
            info!("匹配到唯一城市: {}", matched_cities[0]);
            Some(matched_cities[0].clone())
        }
        _ => {
            // 匹配到多个，调用 LLM 裁决
            info!("匹配到多个城市，调用 LLM 裁决");
            match resolve_city_with_llm(paper_name, Some(province), &matched_cities).await {
                Ok(Some(city)) => Some(city),
                Ok(None) => {
                    warn!("LLM 无法确定城市，使用第一个匹配的城市");
                    Some(matched_cities[0].clone())
                }
                Err(e) => {
                    warn!("LLM 裁决失败: {}，使用第一个匹配的城市", e);
                    Some(matched_cities[0].clone())
                }
            }
        }
    };

    // 3. 如果有城市名称，获取城市 code
    if let Some(city) = city_name {
        let city_code = get_city_code(Some(province), &city);
        if let Some(code) = city_code {
            info!("确定城市: {} (code: {})", city, code);
            Ok(Some(code))
        } else {
            warn!("无法获取城市 '{}' 的 code", city);
            Ok(None)
        }
    } else {
        warn!("无法确定城市");
        Ok(None)
    }
}

/// 构建试卷保存的 payload
async fn build_paper_payload(
    question_page: &QuestionPage,
    attachments: Option<Value>,
) -> Result<Value> {
    // 确定城市
    let city_code =
        determine_city_from_paper_name(&question_page.name, &question_page.province).await?;

    let payload = json!({
        "paperType":"6215",
        "parentPaperType": "ppt4",
        "schNumber": "65",
        "paperYear": String::from(&question_page.year),
        "courseVersionCode": "",
        "address": [
        {
            "province": crate::bank_page_info::address::get_province_code(&question_page.province).unwrap_or_else(||1).to_string(),
            "city": city_code.unwrap_or(0).to_string() // 如果无法确定城市，使用 0
        }
        ],
        "title": &question_page.name,
        "stage": "3",
        "subject": find_subject_code(&question_page.subject).unwrap().to_string(),
        "subjectName": &question_page.subject,
        "stageName": "初中",
        "gradeName": &question_page.grade,
        "grade": find_grade_code(&question_page.grade),
        "schName": "集团",
        "paperId": "",
        "attachments": attachments.unwrap_or_else(|| json!([]))
    });

    Ok(payload)
}

/// 保存试卷到 TOML 文件
fn save_paper_to_toml(question_page: &QuestionPage) -> Result<()> {
    let output_dir = Path::new(OUTPUT_DIR);
    fs::create_dir_all(output_dir)?;
    let toml_path = output_dir.join(format!("{}.toml", question_page.name));
    let toml_content = toml::to_string(question_page)?;
    fs::write(toml_path, toml_content)?;
    Ok(())
}

/// 保存新试卷
pub async fn save_new_paper(
    question_page: &mut QuestionPage,
    tiku_page: &chromiumoxide::Page,
) -> Result<Option<String>> {
    // 上传 PDF 文件
    let pdf_path = format!("{}/{}.pdf", PDF_DIR, question_page.name);
    let attachments = upload_pdf(tiku_page, Path::new(&pdf_path)).await?;
    info!("attachments are:{:?}", &attachments);

    // 构建保存试卷的 payload
    let payload = build_paper_payload(question_page, attachments).await?;
    let payload_json = serde_json::to_string(&payload)?;
    debug!("发送的payload: {}", payload_json);
    debug!(
        "Payload 详细内容: {}",
        serde_json::to_string_pretty(&payload)?
    );

    // 调用保存试卷 API
    let js_code = build_save_paper_js();
    let response_value = execute_js_with_timeout::<SavePaperResponse>(
        tiku_page,
        js_code,
        payload_json,
        "保存试卷请求超时",
    )
    .await?;

    let result: SavePaperResponse = serde_json::from_value(response_value)?;
    debug!("API响应: {}", serde_json::to_string_pretty(&result)?);

    if result.success {
        if let Some(paper_id) = result.data {
            info!("✅ 成功! 获取到的paper_id: {}", paper_id);
            question_page.page_id = Some(paper_id.clone());
            save_paper_to_toml(question_page)?;
            Ok(Some(paper_id))
        } else {
            warn!("❌ API 返回成功但未包含 paper_id");
            Ok(None)
        }
    } else {
        let msg = result
            .message
            .unwrap_or_else(|| "Unknown error".to_string());
        warn!("❌ save failed: {}", msg);
        Ok(None)
    }
}
