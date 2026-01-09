use regex::Regex;

/// 清理文件名中的非法字符
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// 从标题中提取年份
pub fn extract_year(title: &str) -> i32 {
    let re = match Regex::new(r"\d{4}") {
        Ok(re) => re,
        Err(_) => return 2024,
    };

    for cap in re.find_iter(title) {
        if let Ok(year_int) = cap.as_str().parse::<i32>() {
            if (2001..=2030).contains(&year_int) {
                return year_int;
            }
        }
    }
    2024
}
