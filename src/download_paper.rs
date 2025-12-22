use crate::model::{Question, QuestionPage};
use anyhow::{Result, anyhow};
use scraper::{Html, Selector};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// 从页面下载试卷数据并生成 PDF
pub async fn download_page(page: &chromiumoxide::Page) -> Result<QuestionPage> {
    // 提取所有样式和 sec-list 元素的 HTML
    let elements_data_js = r#"
        () => {
            // Get all stylesheets
            const styles = Array.from(document.styleSheets)
                .map(sheet => {
                    try {
                        return Array.from(sheet.cssRules)
                            .map(rule => rule.cssText)
                            .join('\n');
                    } catch (e) {
                        return '';
                    }
                })
                .join('\n');
            
            // Get all sec-list elements
            const sections = Array.from(document.querySelectorAll('.sec-list'));
            return {
                styles: styles,
                elements: sections.map(el => el.outerHTML)
            };
        }
    "#;

    let elements_data: Value = page.evaluate(elements_data_js).await?.into_value()?;

    let elements_array = elements_data["elements"]
        .as_array()
        .ok_or_else(|| anyhow!("无法获取 elements 数组"))?;

    info!("找到 {} 个题目部分。", elements_array.len());

    // 解析题目数据
    let mut questions = Vec::new();
    for element_html in elements_array {
        if let Some(html_str) = element_html.as_str() {
            let document = Html::parse_document(html_str);

            // 查找 exam-item__cnt
            let exam_item_selector =
                Selector::parse(".exam-item__cnt").map_err(|e| anyhow!("选择器解析失败: {}", e))?;
            let origin_selector =
                Selector::parse("a.ques-src").map_err(|e| anyhow!("选择器解析失败: {}", e))?;

            let stem = document
                .select(&exam_item_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "未找到题目".to_string());

            let origin = document
                .select(&origin_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "未找到来源".to_string());

            if !stem.is_empty() && stem != "未找到题目" {
                questions.push(Question {
                    origin,
                    stem,
                    origin_from_our_bank: vec![],
                });
            }
        }
    }

    // 提取标题
    let title_js = r#"
        () => {
            const titleElement = document.querySelector('.title-txt .txt');
            return titleElement ? titleElement.innerText : '未找到标题';
        }
    "#;

    let title_value: Value = page.evaluate(title_js).await?.into_value()?;
    let title: String = title_value.as_str().unwrap_or("未找到标题").to_string();

    // 清理标题中的非法字符
    let title = sanitize_filename(&title);

    // 提取信息（省份、年级）
    let info_js = r#"
        () => {
            const items = document.querySelectorAll('.info-list .item');
            if (items.length >= 2) {
                return {
                    shengfen: items[0].innerText.trim(),
                    nianji: items[1].innerText.trim()
                };
            }
            return { shengfen: '未找到', nianji: '未找到' };
        }
    "#;

    let info: Value = page.evaluate(info_js).await?.into_value()?;

    let province = info["shengfen"].as_str().unwrap_or("未找到").to_string();
    let grade = info["nianji"].as_str().unwrap_or("未找到").to_string();

    // 提取科目
    let subject_js = r#"
        () => {
            const subjectElement = document.querySelector('.subject-menu__title .title-txt');
            return subjectElement ? subjectElement.innerText.trim() : '未找到科目';
        }
    "#;

    let subject_value: Value = page.evaluate(subject_js).await?.into_value()?;
    let subject_text: String = subject_value.as_str().unwrap_or("未找到科目").to_string();

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

    // 从标题中提取年份
    let year = extract_year(&title);

    // 生成 PDF
    let pdf_dir = Path::new("PDF");
    if !pdf_dir.exists() {
        fs::create_dir_all(pdf_dir)?;
    }

    let pdf_path = format!("PDF/{}.pdf", title);

    // 使用 chromiumoxide 的 PDF 功能
    // 注意：chromiumoxide 可能使用不同的 API，这里使用通用的方法
    if let Err(e) = generate_pdf(page, &pdf_path).await {
        warn!("生成 PDF 失败: {}，但继续处理数据", e);
    } else {
        info!("已保存 PDF: {}", pdf_path);
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

/// 清理文件名中的非法字符
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// 从标题中提取年份
fn extract_year(title: &str) -> i32 {
    use regex::Regex;

    // 编译正则表达式，匹配4位数字
    let re = match Regex::new(r"\d{4}") {
        Ok(re) => re,
        Err(_) => return 2024, // 如果编译失败，返回默认年份
    };

    for cap in re.find_iter(title) {
        if let Ok(year_int) = cap.as_str().parse::<i32>() {
            if (2001..=2030).contains(&year_int) {
                return year_int;
            }
        }
    }

    2024 // 默认年份
}

/// 生成 PDF 文件
async fn generate_pdf(page: &chromiumoxide::Page, path: &str) -> Result<()> {
    use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
    use std::path::Path;

    // 创建默认的 PDF 参数
    let params = PrintToPdfParams::default();

    // 使用 save_pdf 方法生成并保存 PDF
    // 注意：生成 PDF 目前仅在 Chrome headless 模式下支持
    let pdf_path = Path::new(path);
    let _pdf_data = page.save_pdf(params, pdf_path).await?;

    // save_pdf 已经将 PDF 保存到文件，并返回 PDF 数据
    // 我们不需要额外操作，函数已经完成了保存
    Ok(())
}
