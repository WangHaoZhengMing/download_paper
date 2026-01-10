use crate::modules::{
    metadata::data_addr::{get_city_code, match_cities_from_paper_name},
    resolve_city_with_llm,
};
use anyhow::Result;
use tracing::{info, warn};

/// 从试卷名称中确定城市（先匹配，如果结果不是1个则调用LLM裁决）
pub async fn determine_city_from_paper_name(
    paper_name: &str,
    province: &str,
) -> Result<Option<i16>> {
    // 1. 先用 Rust 代码匹配城市
    let matched_cities = match_cities_from_paper_name(paper_name, Some(province));

    info!(
        "从试卷名称 '{}' 中匹配到 {} 个城市: {:?}",
        paper_name,
        matched_cities.len(),
        matched_cities
    );

    // 2. 根据匹配结果决定下一步
    let city_name = match matched_cities.len() {
        0 => {
            // 没有匹配到城市
            warn!("未匹配到任何城市");
            None
        }
        1 => {
            // 正好匹配到1个，直接使用
            info!("匹配到唯一城市: {}", matched_cities[0]);
            Some(matched_cities[0].clone())
        }
        _ => {
            // 匹配到多个，调用 LLM 裁决
            info!("匹配到多个城市，调用 LLM 裁决");
            match resolve_city_with_llm(paper_name, Some(province), &matched_cities).await {
                Ok(Some(city)) => Some(city),
                Ok(None) => {
                    warn!("LLM 无法确定城市，使用第一个匹配的城市");
                    Some(matched_cities[0].clone())
                }
                Err(e) => {
                    warn!("LLM 裁决失败: {}，使用第一个匹配的城市", e);
                    Some(matched_cities[0].clone())
                }
            }
        }
    };

    // 3. 如果有城市名称，获取城市 code
    if let Some(city) = city_name {
        let city_code = get_city_code(Some(province), &city);
        if let Some(code) = city_code {
            info!("确定城市: {} (code: {})", city, code);
            Ok(Some(code))
        } else {
            warn!("无法获取城市 '{}' 的 code", city);
            Ok(None)
        }
    } else {
        warn!("无法确定城市");
        Ok(None)
    }
}
