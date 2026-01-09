use chromiumoxide::Page;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error, info};
use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;
use std::io::Write;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub origin: String,
    pub stem: String,
    #[serde(default)]
    pub origin_from_our_bank: Vec<String>,
    pub is_title: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imgs: Option<Vec<String>>,
}

impl Default for Question {
    fn default() -> Self {
        Self {
            origin: String::new(),
            stem: String::new(),
            origin_from_our_bank: Vec::new(),
            is_title: false,
            imgs: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionPage {
    pub name: String,
    pub province: String,
    pub grade: String,
    #[serde(deserialize_with = "deserialize_year")]
    pub year: String,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_id: Option<String>,
    pub stemlist: Vec<Question>,
}

impl QuestionPage {
    /// 返回用于文件系统/COS 上传的安全文件名
    pub fn get_name_for_cos(&self) -> String {
        self.name
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
    }
}

fn deserialize_year<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Visitor;
    use std::fmt;

    struct YearVisitor;

    impl<'de> Visitor<'de> for YearVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or integer representing a year")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }
    }

    deserializer.deserialize_any(YearVisitor)
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperInfo {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub is_exit: bool,
}

impl PaperInfo {
    pub async fn check_paper_existence(&mut self, tiku_page: &Page) -> Result<bool> {
        let paper_title = &self.title;
        let safe_title_json = serde_json::to_string(paper_title).unwrap_or_else(|_| format!("\"{}\"", paper_title));

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

        info!("检查结果: {}", response);
    if let Some(error) = response.get("error") {
        let err_msg = error.as_str().unwrap_or("未知错误");
        error!("API 请求逻辑失败: {}", err_msg);
        return Err(anyhow!("API 请求逻辑失败: {}", err_msg));
    }

    if let Some(data) = response.get("data") {
        if let Some(repeated) = data.get("repeated") {
            if repeated.as_bool().unwrap_or(false) {
                debug!("试卷已存在: {}", paper_title);
                self.is_exit = true;
                let log_path = Path::new("other").join("重复.txt");
                if let Some(parent) = log_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = writeln!(file, "{}", paper_title);
                }
                debug!("已记录重复试卷到日志文件");
                return Ok(true);
            }
        }
    }

    Ok(false)
    }
}