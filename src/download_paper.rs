use crate::model::{Question, QuestionPage};
use anyhow::{Result, anyhow};
use scraper::{Html, Selector};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tracing::{debug, error, info, warn};

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
            
            // Find the container that holds both sec-title and sec-list
            // Usually they are in a common parent like .sec-item or .paper-content
            const container = document.querySelector('.sec-item') || 
                            document.querySelector('.paper-content') || 
                            document.querySelector('body');
            
            if (!container) {
                return { styles: styles, elements: [] };
            }
            
            // Get all sec-title and sec-list elements in DOM order
            const allElements = Array.from(container.querySelectorAll('.sec-title, .sec-list'));
            const elements = [];
            
            allElements.forEach(el => {
                if (el.classList.contains('sec-title')) {
                    // Extract title text from span
                    const span = el.querySelector('span');
                    const titleText = span ? span.innerText.trim() : '';
                    if (titleText) {
                        elements.push({
                            type: 'title',
                            title: titleText,
                            content: ''
                        });
                    }
                } else if (el.classList.contains('sec-list')) {
                    // Extract sec-list content
                    elements.push({
                        type: 'content',
                        title: '',
                        content: el.outerHTML
                    });
                }
            });
            
            return {
                styles: styles,
                elements: elements
            };
        }
    "#;

    debug!("开始提取页面元素数据");
    let elements_data: Value = page.evaluate(elements_data_js).await?.into_value()?;
    debug!("成功获取页面元素数据");

    let elements_array = elements_data["elements"]
        .as_array()
        .ok_or_else(|| {
            error!("无法获取 elements 数组");
            anyhow!("无法获取 elements 数组")
        })?;

    info!("找到 {} 个题目部分。", elements_array.len());

    // 解析题目数据
    let mut questions = Vec::new();
    for element_obj in elements_array {
        let element_type = element_obj["type"]
            .as_str()
            .unwrap_or("");

        if element_type == "title" {
            // 处理标题
            let title = element_obj["title"]
                .as_str()
                .unwrap_or("")
                .to_string();
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
            // 处理题目内容
            let html_str = element_obj["content"]
                .as_str()
                .ok_or_else(|| {
                    error!("无法获取 content 字段");
                    anyhow!("无法获取 content 字段")
                })?;

            let document = Html::parse_document(html_str);

            // 查找 exam-item__cnt（可能有多道题目）
            let exam_item_selector =
                Selector::parse(".exam-item__cnt").map_err(|e| anyhow!("选择器解析失败: {}", e))?;
            let origin_selector =
                Selector::parse("a.ques-src").map_err(|e| anyhow!("选择器解析失败: {}", e))?;

            // 处理该 sec-list 中的所有题目
            for exam_item in document.select(&exam_item_selector) {
                // 提取文本内容作为 stem
                let stem = exam_item
                    .text()
                    .collect::<String>()
                    .trim()
                    .to_string();

                // 提取图片 - 直接在 exam_item 中查找
                let img_selector = Selector::parse("img").map_err(|e| anyhow!("图片选择器解析失败: {}", e))?;
                let mut imgs = Vec::new();
                for img in exam_item.select(&img_selector) {
                    if let Some(src) = img.value().attr("src") {
                        imgs.push(src.to_string());
                    }
                    // 也检查 data-src（懒加载图片）
                    if let Some(data_src) = img.value().attr("data-src") {
                        if !imgs.contains(&data_src.to_string()) {
                            imgs.push(data_src.to_string());
                        }
                    }
                }

                // 查找对应的来源（先在该题目区域内查找，如果找不到则在整文档中查找）
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

    // 提取标题
    let title_js = r#"
        () => {
            const titleElement = document.querySelector('.title-txt .txt');
            return titleElement ? titleElement.innerText : '未找到标题';
        }
    "#;

    debug!("正在提取试卷标题");
    let title_value: Value = page.evaluate(title_js).await?.into_value()?;
    let title: String = title_value.as_str().unwrap_or("未找到标题").to_string();
    debug!("提取到的原始标题: {}", title);

    // 清理标题中的非法字符
    let title = sanitize_filename(&title);
    debug!("清理后的标题: {}", title);

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

    debug!("正在提取省份和年级信息");
    let info: Value = page.evaluate(info_js).await?.into_value()?;

    let province = info["shengfen"].as_str().unwrap_or("未找到").to_string();
    let grade = info["nianji"].as_str().unwrap_or("未找到").to_string();
    debug!("省份: {}, 年级: {}", province, grade);

    // 提取科目
    let subject_js = r#"
        () => {
            const subjectElement = document.querySelector('.subject-menu__title .title-txt');
            return subjectElement ? subjectElement.innerText.trim() : '未找到科目';
        }
    "#;

    debug!("正在提取科目信息");
    let subject_value: Value = page.evaluate(subject_js).await?.into_value()?;
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

    // 从标题中提取年份
    let year = extract_year(&title);
    debug!("提取到的年份: {}", year);

    // 生成 PDF
    debug!("准备生成 PDF 文件");
    let pdf_dir = Path::new("PDF");
    if !pdf_dir.exists() {
        debug!("PDF 目录不存在，正在创建");
        fs::create_dir_all(pdf_dir)?;
    }

    let pdf_path = format!("PDF/{}.pdf", title);
    debug!("PDF 文件路径: {}", pdf_path);

    // 使用 chromiumoxide 的 PDF 功能
    // 注意：chromiumoxide 可能使用不同的 API，这里使用通用的方法
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

    let params = PrintToPdfParams::default();

    let pdf_path = Path::new(path);
    let _pdf_data = page.save_pdf(params, pdf_path).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::connect_to_browser_and_page;
    use crate::download_paper::download_page;
    use tracing::info;
    use std::fs;
    use toml;

    #[tokio::test]
    async fn test_download_paper() {
        // 初始化日志
        crate::logger::init();

        let debug_port = 2001;
        let _total_success = 0;

        info!("🚀 开始试卷下载流程...");
        info!("🔌 浏览器端口: {}", debug_port);

        // 连接到题库平台页面
        let (browser, tiku_page) =
            connect_to_browser_and_page(debug_port, Some("https://zujuan.xkw.com/26p2562957.html"), None)
                .await
                .expect("连接浏览器失败");

        // 下载页面数据
        let result = download_page(&tiku_page).await;
        
        match result {
            Ok(paper) => {
                // 将 paper 序列化为 TOML 格式
                let toml_output = toml::to_string_pretty(&paper)
                    .expect("序列化 paper 失败");
                
                // 写入文件
                fs::write("papaer_debut_output.toml", toml_output)
                    .expect("写入文件失败");
                
                info!("✅ 成功下载试卷: {}", paper.name);
                info!("📄 试卷数据已保存到: papaer_debut_output.toml");
            }
            Err(e) => {
                eprintln!("❌ 下载试卷失败: {}", e);
                // 将错误信息也写入文件
                let error_msg = format!("下载试卷失败: {}\n", e);
                fs::write("papaer_debut_output.txt", error_msg)
                    .expect("写入文件失败");
            }
        }

        drop(browser);
        info!("测试完成");
    }
}

