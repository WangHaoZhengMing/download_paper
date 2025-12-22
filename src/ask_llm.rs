use openai::chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole};
use openai::Credentials;
use tracing::info;

// LLM 配置
const API_KEY: &str = "26e96c4d312e48feacbd78b7c42bd71e";
const API_BASE_URL: &str = "http://menshen.xdf.cn/v1";
const MODEL_NAME: &str = "gemini-3.0-pro-preview"; // 可以根据需要修改模型名称

/// 构建用于裁决城市的 LLM prompt
fn build_city_resolution_prompt(paper_name: &str, province: Option<&str>, matched_cities: &[String]) -> String {
    let province_info = if let Some(prov) = province {
        format!("已知省份：{}\n", prov)
    } else {
        String::new()
    };
    
    let cities_list = matched_cities
        .iter()
        .enumerate()
        .map(|(i, city)| format!("{}. {}", i + 1, city))
        .collect::<Vec<_>>()
        .join("\n");
    
    format!(
        "请根据试卷名称判断应该选择哪个城市。

试卷名称：{}
{}
匹配到的候选城市（{}个）：
{}

请只返回一个最匹配的城市名称，不要包含其他内容。如果无法确定，请返回\"无法确定\"。",
        paper_name,
        province_info,
        matched_cities.len(),
        cities_list
    )
}

/// 调用 LLM API 来裁决城市
pub async fn resolve_city_with_llm(
    paper_name: &str,
    province: Option<&str>,
    matched_cities: &[String],
) -> anyhow::Result<Option<String>> {
    if matched_cities.is_empty() {
        return Ok(None);
    }
    
    info!("使用 LLM 裁决城市，试卷名称: {}, 候选城市数量: {}", paper_name, matched_cities.len());
    
    let prompt = build_city_resolution_prompt(paper_name, province, matched_cities);
    
    let credentials = Credentials::new(API_KEY, API_BASE_URL);
    
    let messages = vec![
        ChatCompletionMessage {
            role: ChatCompletionMessageRole::System,
            content: Some("你是一个专业的城市识别助手，能够根据试卷名称准确识别城市。".to_string()),
            name: None,
            function_call: None,
            tool_call_id: None,
            tool_calls: None,
        },
        ChatCompletionMessage {
            role: ChatCompletionMessageRole::User,
            content: Some(prompt),
            name: None,
            function_call: None,
            tool_call_id: None,
            tool_calls: None,
        },
    ];

    let chat_completion = ChatCompletion::builder(MODEL_NAME, messages)
        .credentials(credentials)
        .create()
        .await
        .map_err(|e| anyhow::anyhow!("LLM API 调用失败: {}", e))?;

    let returned_message = chat_completion
        .choices
        .first()
        .ok_or_else(|| anyhow::anyhow!("LLM 返回结果为空"))?
        .message
        .clone();
    
    let content = returned_message
        .content
        .ok_or_else(|| anyhow::anyhow!("LLM 返回内容为空"))?;
    
    let city_name = content.trim();
    
    // 检查返回的城市是否在候选列表中
    if city_name == "无法确定" || city_name.is_empty() {
        info!("LLM 无法确定城市");
        return Ok(None);
    }
    
    // 检查返回的城市是否在候选列表中（支持带"市"或不带"市"）
    for matched_city in matched_cities {
        if city_name == matched_city || city_name == matched_city.trim_end_matches("市") {
            info!("LLM 裁决结果: {}", matched_city);
            return Ok(Some(matched_city.clone()));
        }
    }
    
    // 如果返回的城市不在候选列表中，尝试直接匹配
    info!("LLM 返回的城市 '{}' 不在候选列表中，尝试直接使用", city_name);
    Ok(Some(city_name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_city_resolution_prompt_with_province() {
        let paper_name = "2024年浙江省杭州市中考数学试卷";
        let province = Some("浙江省");
        let matched_cities = vec!["杭州市".to_string(), "宁波市".to_string(), "温州市".to_string()];
        
        let prompt = build_city_resolution_prompt(paper_name, province, &matched_cities);
        
        // 检查 prompt 包含必要信息
        assert!(prompt.contains(paper_name), "prompt 应该包含试卷名称");
        assert!(prompt.contains("已知省份：浙江省"), "prompt 应该包含省份信息");
        assert!(prompt.contains("杭州市"), "prompt 应该包含候选城市");
        assert!(prompt.contains("宁波市"), "prompt 应该包含候选城市");
        assert!(prompt.contains("温州市"), "prompt 应该包含候选城市");
        assert!(prompt.contains("3个"), "prompt 应该包含城市数量");
    }

    #[test]
    fn test_build_city_resolution_prompt_without_province() {
        let paper_name = "2024年北京市中考数学试卷";
        let province = None;
        let matched_cities = vec!["北京市".to_string()];
        
        let prompt = build_city_resolution_prompt(paper_name, province, &matched_cities);
        
        // 检查 prompt 不包含省份信息
        assert!(prompt.contains(paper_name), "prompt 应该包含试卷名称");
        assert!(!prompt.contains("已知省份"), "没有省份时不应该包含省份信息");
        assert!(prompt.contains("北京市"), "prompt 应该包含候选城市");
    }

    #[test]
    fn test_build_city_resolution_prompt_empty_cities() {
        let paper_name = "2024年某地中考数学试卷";
        let province = Some("浙江省");
        let matched_cities: Vec<String> = vec![];
        
        let prompt = build_city_resolution_prompt(paper_name, province, &matched_cities);
        
        assert!(prompt.contains(paper_name), "prompt 应该包含试卷名称");
        assert!(prompt.contains("0个"), "prompt 应该显示城市数量为0");
    }

    #[tokio::test]
    async fn test_resolve_city_with_llm_empty_cities() {
        // 测试空城市列表的情况
        let result = resolve_city_with_llm("测试试卷", Some("浙江省"), &[]).await;
        
        assert!(result.is_ok(), "应该成功返回");
        assert_eq!(result.unwrap(), None, "空列表应该返回 None");
    }

    #[tokio::test]
    #[ignore] // 标记为忽略，因为需要真实的 API 调用
    async fn test_resolve_city_with_llm_single_city() {
        // 这是一个集成测试，需要真实的 API 调用
        // 在实际使用时可以取消 ignore 标记
        let matched_cities = vec!["杭州市".to_string()];
        let result = resolve_city_with_llm(
            "2024年浙江省杭州市中考数学试卷",
            Some("浙江省"),
            &matched_cities,
        ).await;
        
        // 由于是真实 API 调用，结果可能不确定，只检查不会 panic
        let _ = result;
    }
}