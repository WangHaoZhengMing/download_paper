use anyhow::{anyhow, Result};
use chromiumoxide::Page;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use tracing::{debug, error, info};

use crate::core::models::QuestionPage;
use crate::utils::text::sanitize_filename;

// /// 调用题库接口检查试卷是否存在
// pub async fn check_paper_exists(tiku_page: &Page, paper_title: &str) -> Result<bool> {
//     let safe_title_json = serde_json::to_string(paper_title).unwrap_or_else(|_| format!("\"{}\"", paper_title));

//     let check_js = format!(
//         r#"
//         (async () => {{
//             try {{
//                 const rawTitle = {0}; 
//                 const paperName = encodeURIComponent(rawTitle);
//                 const url = `https://tps-tiku-api.staff.xdf.cn/paper/check/paperName?paperName=${{paperName}}&operationType=1&paperId=`;
//                 const response = await fetch(url, {{
//                     method: "GET",
//                     headers: {{
//                         "Accept": "application/json, text/plain, */*"
//                     }},
//                     credentials: "include"
//                 }});
//                 if (!response.ok) {{
//                     return {{ error: `HTTP Error: ${{response.status}}` }};
//                 }}
//                 const data = await response.json();
//                 return data;
//             }} catch (err) {{
//                 return {{ error: err.toString() }};
//             }}
//         }})()
//         "#,
//         safe_title_json
//     );

//     info!("检查试卷是否已存在: {}", paper_title);

//     let response: Value = tiku_page
//         .evaluate(check_js)
//         .await
//         .map_err(|e| {
//             error!("执行检查脚本失败: {}", e);
//             e
//         })?
//         .into_value()
//         .map_err(|e| {
//             error!("解析脚本返回值失败: {}", e);
//             anyhow!("解析脚本返回值失败: {}", e)
//         })?;

//     if let Some(error) = response.get("error") {
//         let err_msg = error.as_str().unwrap_or("未知错误");
//         error!("API 请求逻辑失败: {}", err_msg);
//         return Err(anyhow!("API 请求逻辑失败: {}", err_msg));
//     }

//     if let Some(data) = response.get("data") {
//         if let Some(repeated) = data.get("repeated") {
//             if repeated.as_bool().unwrap_or(false) {
//                 debug!("试卷已存在: {}", paper_title);
//                 let log_path = Path::new("other").join("重复.txt");
//                 if let Some(parent) = log_path.parent() {
//                     let _ = fs::create_dir_all(parent);
//                 }
//                 if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
//                     let _ = writeln!(file, "{}", paper_title);
//                 }
//                 debug!("已记录重复试卷到日志文件");
//                 return Ok(true);
//             }
//         }
//     }

//     Ok(false)
// }
 
/// 本地保存试卷 TOML（占位，可扩展上传逻辑）
pub fn persist_paper_locally(question_page: &QuestionPage, output_dir: &str) -> Result<()> {
    let output_dir = Path::new(output_dir);
    fs::create_dir_all(output_dir)?;
    let sanitized_name = sanitize_filename(&question_page.name);
    let toml_path = output_dir.join(format!("{}.toml", sanitized_name));
    let toml_content = toml::to_string(question_page)?;
    fs::write(toml_path, toml_content)?;
    Ok(())
}
