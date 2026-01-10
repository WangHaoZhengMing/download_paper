use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;

const API_BASE_URL: &str = "https://tps-tiku-api.staff.xdf.cn";
const CREDENTIAL_API_PATH: &str = "/attachment/get/credential";
const NOTIFY_API_PATH: &str = "/attachment/batch/upload/files";
const SAVE_PAPER_API_PATH: &str = "/paper/new/save";
const TIKU_TOKEN: &str = "732FD8402F95087CD934374135C46EE5";
const JS_TIMEOUT_SECS: u64 = 16;

pub const ELEMENTS_DATA_JS: &str = r#"
        () => {
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
            const container = document.querySelector('.sec-item') ||
                            document.querySelector('.paper-content') ||
                            document.querySelector('body');
            if (!container) {
                return { styles: styles, elements: [] };
            }
            const allElements = Array.from(container.querySelectorAll('.sec-title, .sec-list'));
            const elements = [];
            allElements.forEach(el => {
                if (el.classList.contains('sec-title')) {
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
                    elements.push({
                        type: 'content',
                        title: '',
                        content: el.outerHTML
                    });
                }
            });
            return { styles: styles, elements: elements };
        }
    "#;

pub const TITLE_JS: &str = r#"
        () => {
            const titleElement = document.querySelector('.title-txt .txt');
            return titleElement ? titleElement.innerText : '未找到标题';
        }
    "#;

pub const INFO_JS: &str = r#"
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

pub const SUBJECT_JS: &str = r#"
        () => {
            const subjectElement = document.querySelector('.subject-menu__title .title-txt');
            return subjectElement ? subjectElement.innerText.trim() : '未找到科目';
        }
    "#;

/// 生成获取上传凭证的 JavaScript 代码
pub fn build_credential_request_js() -> String {
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

/// 执行 JavaScript 代码并处理超时
pub async fn execute_js_with_timeout<T>(
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

/// 生成通知应用服务器的 JavaScript 代码
pub fn build_notify_server_js() -> String {
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
pub fn build_save_paper_js(playload: &str) -> String {
    format!(
        r#"
        (async () => {{
            try {{
                const response = await fetch("{API_BASE_URL}{SAVE_PAPER_API_PATH}", {{
                    method: "POST",
                    headers: {{
                        "Content-Type": "application/json",
                        "Accept": "application/json, text/plain, */*",
                        "tikutoken": "732FD8402F95087CD934374135C46EE5"
                    }},
                    credentials: "include",
                    body: JSON.stringify({}),
                }});
                const data = await response.json();
                return data;
            }} catch (err) {{
                return {{ error: err.toString() }};
            }}
        }})()
        "#,
        playload
    )
}
