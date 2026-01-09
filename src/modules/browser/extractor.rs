use anyhow::{anyhow, Result};
use chromiumoxide::Page;
use scraper::{Html, Selector};
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::core::models::{Question, QuestionPage};
use crate::modules::browser::actions::generate_pdf;
use crate::modules::browser::scripts::{ELEMENTS_DATA_JS, INFO_JS, SUBJECT_JS, TITLE_JS};
use crate::utils::text::{extract_year, sanitize_filename};
use std::fs;
use std::path::Path;

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

    let pdf_path = format!("PDF/{}.pdf", title);
    debug!("PDF 文件路径: {}", pdf_path);

    debug!("开始生成 PDF");
    if let Err(e) = generate_pdf(page, &pdf_path).await {
        error!("生成 PDF 失败: {}，但继续处理数据", e);
        warn!("生成 PDF 失败: {}，但继续处理数据", e);
    } else {
        info!("已保存 PDF: {}", pdf_path);
        debug!("PDF 生成成功");
    }

    Ok(QuestionPage {
        name: title,
        province,
        grade,
        year: year.to_string(),
        subject,
        page_id: None,
        stemlist: questions,
    })
}
