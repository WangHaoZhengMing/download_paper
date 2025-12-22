use phf::phf_map;

// 科目映射（科目名称 -> 科目代码）
static SUBJECT_MAP: phf::Map<&'static str, i16> = phf_map! {
    "语文" => 53,
    "数学" => 54,
    "英语" => 55,
    "物理" => 56,
    "化学" => 57,
    "生物" => 58,
    "历史" => 59,
    "政治" => 60,
    "地理" => 61,
    "科学" => 62,
};

/// 获取科目code
pub fn get_subject_code(subject_name: &str) -> Option<i16> {
    SUBJECT_MAP.get(subject_name).copied()
}

/// 智能查找科目code（支持简写）
pub fn find_subject_code(name: &str) -> Option<i16> {
    // 先尝试完整名称
    if let Some(code) = get_subject_code(name) {
        return Some(code);
    }
    
    // 尝试简写匹配
    let simplified_map: phf::Map<&'static str, i16> = phf_map! {
        "语" => 53,
        "数" => 54,
        "英" => 55,
        "物" => 56,
        "化" => 57,
        "生" => 58,
        "历" => 59,
        "政" => 60,
        "地" => 61,
        "科" => 62,
    };
    
    simplified_map.get(name).copied()
}
