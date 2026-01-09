use anyhow::{Result, anyhow};
use std::path::Path;


/// 清理试卷名称，去掉文件系统不支持的字符（用于 name 字段）
/// 只去掉 /\ 这种文件系统不支持的字符
pub fn sanitize_paper_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' => '_', // 只替换文件系统不支持的斜杠
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// 清理文件名用于 COS 上传（用于 name_for_cos 字段）
/// 转换特殊字符，确保可以安全用于上传
pub fn sanitize_filename_for_cos(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            // Windows文件系统不支持的字符
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            // URL特殊字符，可能导致问题
            '+' | '&' | '=' | '%' | '#' | '@' | '!' | '$' | '`' | '~' => '_',
            // 控制字符和不可见字符
            c if c.is_control() => '_',
            // 其他特殊Unicode字符（保留中文字符和常见标点）
            c if c.is_whitespace() => '_', // 空格、制表符等替换为下划线
            _ => c,
        })
        .collect::<String>()
        .trim() // 去除前后空白
        .to_string()
}

/// 清理文件名中的非法字符和特殊符号（用于本地文件系统）
/// 处理Windows文件系统不支持的字符，以及URL中可能有问题的字符
/// 与 download_paper.rs 中的 sanitize_filename 保持一致
pub fn sanitize_filename(filename: &str) -> String {
    sanitize_filename_for_cos(filename)
}

/// 清理 LLM 返回的 JSON 字符串，去除 markdown 代码块标记
pub fn clean_json_string(input: &str) -> &str {
    // 1. 尝试寻找 ```json 和 ``` 包裹的内容
    if let Some(start) = input.find("```json") {
        if let Some(end) = input[start..].find("```") {
            // 注意：这里可能会找到同一个起始符，需要处理
            // 更严谨的逻辑：
            let start_index = start + 7; // 跳过 ```json
            if let Some(end_offset) = input[start_index..].rfind("```") {
                let end_index = start_index + end_offset;
                return input[start_index..end_index].trim();
            }
        }
    }

    // 2. 如果没有 json 标签，尝试找普通的 ```
    if let Some(start) = input.find("```") {
        let start_index = start + 3;
        if let Some(end_offset) = input[start_index..].rfind("```") {
            let end_index = start_index + end_offset;
            return input[start_index..end_index].trim();
        }
    }

    // 3. 如果没有代码块，尝试寻找第一个 { 和最后一个 }
    if let Some(start) = input.find('{') {
        if let Some(end) = input.rfind('}') {
            if start < end {
                return input[start..=end].trim();
            }
        }
    }

    // 4. 如果都失败了，直接返回原字符串（Trim一下）
    input.trim()
}
