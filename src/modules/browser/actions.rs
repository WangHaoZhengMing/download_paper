use anyhow::{Result, anyhow};
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use scraper::{Html, Selector};
use serde_json::{Value, json};
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::core::models::{Question, QuestionPage};
use crate::modules::browser::scripts::{ELEMENTS_DATA_JS, INFO_JS, SUBJECT_JS, TITLE_JS};
use crate::modules::{build_credential_request_js, build_notify_server_js, build_save_paper_js, execute_js_with_timeout};
use crate::modules::cos_client::{CosUploader, TempCredentials};
use crate::modules::credential::{CredentialData, CredentialResponse, FileInfo, NotifyResponse};
use crate::utils::text::{extract_year, sanitize_filename};
use std::fs;

/// 生成 PDF 文件
pub async fn generate_pdf(page: &chromiumoxide::Page, path: &str) -> Result<()> {
    let params = PrintToPdfParams::default();
    let pdf_path = Path::new(path);
    let _pdf_data = page.save_pdf(params, pdf_path).await?;
    Ok(())
}

/// 从页面下载试卷数据并生成 PDF
pub async fn download_page(page: &Page) -> Result<QuestionPage> {
    debug!("开始提取页面元素数据");
    let elements_data: Value = page.evaluate(ELEMENTS_DATA_JS).await?.into_value()?;
    debug!("成功获取页面元素数据");

    let elements_array = elements_data["elements"].as_array().ok_or_else(|| {
        error!("无法获取 elements 数组");
        anyhow!("无法获取 elements 数组")
    })?;

    info!("找到 {} 个题目部分。", elements_array.len());

    let mut questions = Vec::new();
    for element_obj in elements_array {
        let element_type = element_obj["type"].as_str().unwrap_or("");

        if element_type == "title" {
            let title = element_obj["title"].as_str().unwrap_or("").to_string();
            if !title.is_empty() {
                debug!("处理章节: {}", title);
                questions.push(Question {
                    origin: String::new(),
                    stem: title,
                    origin_from_our_bank: vec![],
                    is_title: true,
                    imgs: None,
                });
            }
        } else if element_type == "content" {
            let html_str = element_obj["content"].as_str().ok_or_else(|| {
                error!("无法获取 content 字段");
                anyhow!("无法获取 content 字段")
            })?;

            let document = Html::parse_document(html_str);

            let exam_item_selector =
                Selector::parse(".exam-item__cnt").map_err(|e| anyhow!("选择器解析失败: {}", e))?;
            let origin_selector =
                Selector::parse("a.ques-src").map_err(|e| anyhow!("选择器解析失败: {}", e))?;

            for exam_item in document.select(&exam_item_selector) {
                let stem = exam_item.text().collect::<String>().trim().to_string();

                let img_selector =
                    Selector::parse("img").map_err(|e| anyhow!("图片选择器解析失败: {}", e))?;
                let mut imgs = Vec::new();
                for img in exam_item.select(&img_selector) {
                    if let Some(src) = img.value().attr("src") {
                        imgs.push(src.to_string());
                    }
                    if let Some(data_src) = img.value().attr("data-src") {
                        if !imgs.contains(&data_src.to_string()) {
                            imgs.push(data_src.to_string());
                        }
                    }
                }

                let origin = exam_item
                    .select(&origin_selector)
                    .next()
                    .or_else(|| document.select(&origin_selector).next())
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_else(|| "未找到来源".to_string());

                if !stem.is_empty() && stem != "未找到题目" {
                    questions.push(Question {
                        origin,
                        stem,
                        origin_from_our_bank: vec![],
                        is_title: false,
                        imgs: if imgs.is_empty() { None } else { Some(imgs) },
                    });
                }
            }
        }
    }

    debug!("正在提取试卷标题");
    let title_value: Value = page.evaluate(TITLE_JS).await?.into_value()?;
    let title: String = title_value.as_str().unwrap_or("未找到标题").to_string();
    debug!("提取到的原始标题: {}", title);

    let title = sanitize_filename(&title);
    debug!("清理后的标题: {}", title);

    debug!("正在提取省份和年级信息");
    let info: Value = page.evaluate(INFO_JS).await?.into_value()?;
    let province = info["shengfen"].as_str().unwrap_or("未找到").to_string();
    let grade = info["nianji"].as_str().unwrap_or("未找到").to_string();
    debug!("省份: {}, 年级: {}", province, grade);

    debug!("正在提取科目信息");
    let subject_value: Value = page.evaluate(SUBJECT_JS).await?.into_value()?;
    let subject_text: String = subject_value.as_str().unwrap_or("未找到科目").to_string();
    debug!("提取到的科目文本: {}", subject_text);

    let valid_subjects = [
        "语文", "数学", "英语", "物理", "化学", "生物", "历史", "政治", "地理", "科学",
    ];
    let mut subject = "未知".to_string();
    for s in &valid_subjects {
        if subject_text.contains(s) {
            subject = s.to_string();
            break;
        }
    }
    debug!("识别到的科目: {}", subject);

    let year = extract_year(&title);
    debug!("提取到的年份: {}", year);

    debug!("准备生成 PDF 文件");
    let pdf_dir = Path::new("PDF");
    if !pdf_dir.exists() {
        debug!("PDF 目录不存在，正在创建");
        fs::create_dir_all(pdf_dir)?;
    }
    let name_for_pdf = sanitize_filename(&title);
    let pdf_path = format!("PDF/{}.pdf", name_for_pdf);
    debug!("PDF 文件路径: {}", pdf_path);

    debug!("开始生成 PDF");
    if let Err(e) = generate_pdf(page, &pdf_path).await {
        error!("生成 PDF 失败: {}，但继续处理数据", e);
        warn!("生成 PDF 失败: {}，但继续处理数据", e);
    } else {
        info!("已保存 PDF: {}", pdf_path);
        debug!("PDF 生成成功");
    }
// ============================================================================

    Ok(QuestionPage {
        name: title,
        province,
        grade,
        year: year.to_string(),
        subject,
        page_id: None,
        stemlist: questions,
        name_for_pdf,
    })
}




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
        debug!("凭证数据: {:?}", response.data);
        Ok(response.data.unwrap())
    } else {
        let msg = response
            .message
            .unwrap_or_else(|| "Unknown error".to_string());
        error!("❌ API响应格式不正确或未成功: {}", msg);
        Err(anyhow!("Failed to get credentials: {}", msg))
    }
}

/// 阶段2: 上传文件到腾讯云COS
async fn upload_to_cos(credentials_data: CredentialData, file_path: &Path) -> Result<FileInfo> {
    info!("--- 阶段2: 正在上传文件到腾讯云COS... ---");

    let temp_creds = TempCredentials {
        region: credentials_data.region,
        bucket: credentials_data.bucket,
        key_prefix: credentials_data.key_prefix,
        cdn_domain: credentials_data.cdn_domain,
        tmp_secret_id: credentials_data.credentials.tmp_secret_id,
        tmp_secret_key: credentials_data.credentials.tmp_secret_key,
        session_token: credentials_data.credentials.session_token,
    };

    let uploader = CosUploader::from_temp_credentials(temp_creds);
    let file_info = uploader.upload(file_path).await?;

    info!("✅ 文件上传成功。");
    info!("最终文件URL: {}", file_info.url);
    debug!("文件上传完成，URL: {}", file_info.url);

    Ok(file_info)
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
    debug!("通知响应: {:?}", response);
    Ok(response)
}

// ============================================================================
// 公共 API
// ============================================================================

/// 上传 PDF 文件并通知服务器（完整流程）
pub async fn upload_pdf_to_server(
    page: &chromiumoxide::Page,
    file_path: &Path,
) -> Result<Option<Value>> {
    if !file_path.exists() {
        return Err(anyhow!("文件不存在: {:?}", file_path));
    }

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("无法从路径中提取文件名: {:?}", file_path))?;

    let credentials = get_upload_credentials(page, filename).await?;
    let file_info = upload_to_cos(credentials, file_path).await?;
    let notify_response = notify_application_server(page, filename, &file_info).await?;

    if notify_response.success && notify_response.data.is_some() {
        info!("{}", "=".repeat(50));
        info!("🎉 成功获取到目标 `data` 数组! 🎉");
        let data = notify_response.data.clone();
        debug!("附件数据: {:?}", data);
        Ok(data)
    } else {
        warn!("未能从最终响应中找到 'data' 数组");
        error!("上传流程完成但未获取到附件数据");
        Ok(None)
    }
}

 
