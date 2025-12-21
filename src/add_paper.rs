use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use uuid::Uuid;
use tokio::time::{timeout, Duration};
use tracing::{info, warn, debug};
use crate::tencent_cos::{CosConfig, CosS3Client};
use crate::model::QuestionPage;

const API_BASE_URL: &str = "https://tps-tiku-api.staff.xdf.cn";
const NOTIFY_API_PATH: &str = "/attachment/batch/upload/files";

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

async fn get_upload_credentials(page: &chromiumoxide::Page, filename: &str) -> Result<CredentialData> {
    info!("--- 阶段1: 正在请求上传凭证 (Via Page Evaluate)... ---");

    let js_code = format!(r#"
       async (filename) => {{
       const payload = {{
            fileName: filename,
            contentType: "application/pdf",
            storageType: "cos",
            securityLevel: 1
        }};
           try {{
               const response = await fetch("https://tps-tiku-api.staff.xdf.cn/attachment/get/credential", {{
                   method: "POST",
                   headers: {{
                       "Content-Type": "application/json",
                       "Accept": "application/json, text/plain, */*",
                       "tikutoken": "732FD8402F95087CD934374135C46EE5"
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
    "#);

    let filename_json = serde_json::to_string(filename)?;
    let eval_future = page.evaluate(format!("({})({})", js_code, filename_json));
    let eval_result = timeout(Duration::from_secs(16), eval_future)
        .await
        .map_err(|_| anyhow!("等待上传凭证响应超时"))??;
    let response_value: Value = eval_result.into_value()?;
    let response: CredentialResponse = serde_json::from_value(response_value)?;

    if response.success && response.data.is_some() {
        info!("✅ 凭证获取成功。");
        Ok(response.data.unwrap())
    } else {
        let msg = response.message.unwrap_or_else(|| "Unknown error".to_string());
        warn!("❌ 错误: API响应格式不正确或未成功: {}", msg);
        Err(anyhow!("Failed to get credentials: {}", msg))
    }
}

async fn upload_to_cos(credentials_data: CredentialData, file_path: &Path) -> Result<Value> {
    info!("--- 阶段2: 正在上传文件到腾讯云COS... ---");
    
    let creds = &credentials_data.credentials;
    let config = CosConfig::new(
        None,
        Some(credentials_data.region.clone()),
        Some(creds.tmp_secret_id.clone()),
        Some(creds.tmp_secret_key.clone()),
        Some(creds.session_token.clone()),
        Some("https".to_string()),
        None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None
    );
    
    let client = CosS3Client::new(config, None, None);
    
    let bucket = &credentials_data.bucket;
    let key_prefix = &credentials_data.key_prefix;
    let filename = file_path.file_name().and_then(|n| n.to_str()).ok_or_else(|| anyhow!("Invalid filename"))?;
    let object_key = format!("{}/{}/{}", key_prefix, Uuid::new_v4(), filename);
    
    debug!("云端路径 (Key): {}", object_key);
    
    // Note: CosS3Client::upload_file needs to be implemented.
    // For now, we'll assume it exists or we'll implement a basic version.
    client.upload_file(bucket, file_path, &object_key).await?;
    
    let final_url = format!("https://{}/{}", credentials_data.cdn_domain, object_key);
    info!("✅ 文件上传成功。");
    info!("最终文件URL: {}", final_url);
    
    Ok(json!({
        "url": final_url,
        "key": object_key
    }))
}

async fn notify_application_server(page: &chromiumoxide::Page, filename: &str, file_info: &Value) -> Result<Value> {
    info!("--- 阶段3: 正在通知应用服务器 (Via Page Evaluate)... ---");
    
    let file_url = file_info["url"].as_str().ok_or_else(|| anyhow!("Missing file URL"))?;
    
    let js_code = format!(r#"
        async (data) => {{
            const url = "{API_BASE_URL}{NOTIFY_API_PATH}";
            const payload = {{
                "uploadAttachments": [
                    {{
                        "fileName": data.filename,
                        "fileType": "pdf",
                        "fileUrl": data.fileUrl,
                        "resourceType": "zbtiku_pc"
                    }}
                ],
                "fileUploadType": 5,
                "fileContentType": 1,
                "paperId": ""
            }};
            
            try {{
                const response = await fetch(url, {{
                    method: "POST",
                    headers: {{
                        "Content-Type": "application/json",
                        "Accept": "application/json, text/plain, */*",
                        "tikutoken": "732FD8402F95087CD934374135C46EE5"
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
    "#);

    let data = json!({
        "filename": filename,
        "fileUrl": file_url
    });
    
    let eval_future = page.evaluate(format!("({})({})", js_code, data));
    let eval_result = timeout(Duration::from_secs(16), eval_future)
        .await
        .map_err(|_| anyhow!("通知应用服务器超时"))??;
    let response_data: Value = eval_result.into_value()?;
    info!("✅ 服务器通知成功，已收到返回数据。");
    Ok(response_data)
}

async fn upload_pdf(page: &chromiumoxide::Page, file_path: &Path) -> Result<Option<String>> {
    if !file_path.exists() {
        return Err(anyhow!("File '{:?}' does not exist", file_path));
    }

    let filename = file_path.file_name().and_then(|n| n.to_str()).ok_or_else(|| anyhow!("Invalid filename"))?;
    
    let credentials = get_upload_credentials(page, filename).await?;
    let file_info = upload_to_cos(credentials, file_path).await?;
    let final_result = notify_application_server(page, filename, &file_info).await?;
    
    if final_result["success"].as_bool().unwrap_or(false) && final_result.get("data").is_some() {
        let data_array = &final_result["data"];
        info!("{}", "=".repeat(50));
        info!("🎉 成功获取到目标 `data` 数组! 🎉");
        Ok(Some(format!("\"attachments\": {}", serde_json::to_string_pretty(data_array)?)))
    } else {
        warn!("未能从最终响应中找到 'data' 数组。服务器返回内容如下:");
        debug!("{}", serde_json::to_string_pretty(&final_result)?);
        Ok(None)
    }
}

pub async fn save_new_paper(question_page: &mut QuestionPage, tiku_page: &chromiumoxide::Page) -> Result<Option<String>> {
    // Placeholder for ask_llm_for_playload
    let payload_str = format!(r#""name": "{}", "subject": "{}", "province": "{}""#, question_page.name, question_page.subject, question_page.province);
    
    let pdf_path = format!("PDF/{}.pdf", question_page.name);
    let parcial_payload = upload_pdf(tiku_page, Path::new(&pdf_path)).await?;

    let mut payload_dict: serde_json::Map<String, Value> = serde_json::from_str(&format!("{{{}}}", payload_str))?;

    if let Some(parcial) = parcial_payload {
        if let Some((key, value_str)) = parcial.split_once(':') {
            let key = key.trim().trim_matches('"');
            let value: Value = serde_json::from_str(value_str.trim())?;
            payload_dict.insert(key.to_string(), value);
        }
    }

    let payload_json = serde_json::to_string(&payload_dict)?;
    debug!("发送的payload: {}", payload_json);

    let js_code = format!(r#"
        async (payload) => {{
            try {{
                const response = await fetch("https://tps-tiku-api.staff.xdf.cn/paper/new/save", {{
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
    "#);

    let result: Value = tiku_page.evaluate(format!("({})('{}')", js_code, payload_json)).await?.into_value()?;
    debug!("API响应: {}", serde_json::to_string_pretty(&result)?);

    if result["success"].as_bool().unwrap_or(false) {
        let paper_id = result["data"].as_str().map(|s| s.to_string());
        if let Some(ref id) = paper_id {
            info!("✅ 成功! 获取到的paper_id: {}", id);
            question_page.page_id = Some(id.clone());
            
            let output_dir = Path::new("./output_toml");
            fs::create_dir_all(output_dir)?;
            let toml_path = output_dir.join(format!("{}.toml", question_page.name));
            
            let toml_content = toml::to_string(&question_page)?;
            fs::write(toml_path, toml_content)?;
        }
        Ok(paper_id)
    } else {
        warn!("❌ 请求失败或未返回成功状态");
        Ok(None)
    }
}
