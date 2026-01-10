use phf::phf_map;

// 年级映射（年级名称 -> 年级代码）
static GRADE_MAP: phf::Map<&'static str, i16> = phf_map! {
    "七年级" => 161,
    "八年级" => 162,
    "九年级" => 163,
    // 支持其他常见写法
    "初一" => 161,
    "初二" => 162,
    "初三" => 163,
    "7年级" => 161,
    "8年级" => 162,
    "9年级" => 163,
};

/// 获取年级code
pub fn get_grade_code(grade_name: &str) -> Option<i16> {
    GRADE_MAP.get(grade_name).copied()
}

/// 智能查找年级code（支持多种格式）
pub fn find_grade_code(name: &str) -> Option<i16> {
    // 先尝试直接匹配
    if let Some(code) = get_grade_code(name) {
        return Some(code);
    }
    
    // 尝试从字符串中提取年级信息
    if name.contains("七") || name.contains("7") || name.contains("初一") {
        return Some(161);
    }
    if name.contains("八") || name.contains("8") || name.contains("初二") {
        return Some(162);
    }
    if name.contains("九") || name.contains("9") || name.contains("初三") {
        return Some(163);
    }
    
    None
}
