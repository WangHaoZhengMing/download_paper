use anyhow::{Result, anyhow};
use chromiumoxide::Page;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use tracing::{debug, error, info};

/// 检查试卷是否已存在
pub async fn check_paper_exists(tiku_page: &Page, paper_title: &str) -> Result<bool> {

    let safe_title_json = serde_json::to_string(paper_title)
        .unwrap_or_else(|_| format!("\"{}\"", paper_title));

    let check_js = format!(
        r#"
        (async () => {{
            try {{
                const rawTitle = {0}; 
                const paperName = encodeURIComponent(rawTitle);
                const url = `https://tps-tiku-api.staff.xdf.cn/paper/check/paperName?paperName=${{paperName}}&operationType=1&paperId=`;
                
                const response = await fetch(url, {{
                    method: "GET",
                    headers: {{
                        "Accept": "application/json, text/plain, */*"
                    }},
                    credentials: "include"
                }});

                if (!response.ok) {{
                    return {{ error: `HTTP Error: ${{response.status}}` }};
                }}

                const data = await response.json();
                return data;
            }} catch (err) {{
                return {{ error: err.toString() }};
            }}
        }})()
        "#,
        safe_title_json
    );

    info!("检查试卷是否已存在: {}", paper_title);

    // 3. 执行脚本
    // chromiumoxide 的 evaluate 默认会自动等待 Promise (awaitPromise=true)
    let response: Value = tiku_page
        .evaluate(check_js)
        .await
        .map_err(|e| {
            error!("执行检查脚本失败: {}", e);
            e
        })?
        .into_value()
        .map_err(|e| {
            error!("解析脚本返回值失败: {}", e);
            anyhow!("解析脚本返回值失败: {}", e)
        })?;

    // 4. 检查 API 是否返回了错误字段
    if let Some(error) = response.get("error") {
        let err_msg = error.as_str().unwrap_or("未知错误");
        error!("API 请求逻辑失败: {}", err_msg);
        return Err(anyhow!("API 请求逻辑失败: {}", err_msg));
    }

    // 5. 解析业务数据
    if let Some(data) = response.get("data") {
        if let Some(repeated) = data.get("repeated") {
            // 这里的 repeated 可能是 boolean，也可能是 null
            if repeated.as_bool().unwrap_or(false) {
                debug!("试卷已存在: {}", paper_title);

                // --- 记录日志逻辑 ---
                let log_path = Path::new("other").join("重复.txt");
                if let Some(parent) = log_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    let _ = writeln!(file, "{}", paper_title);
                }
                debug!("已记录重复试卷到日志文件");
                // ------------------

                return Ok(true);
            }
        }
    }

    Ok(false)
}

