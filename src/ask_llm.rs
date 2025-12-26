use openai::chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole};
use openai::Credentials;
use tracing::{debug, info, warn};

// LLM 配置
const API_KEY: &str = "26e96c4d312e48feacbd78b7c42bd71e";
const API_BASE_URL: &str = "http://menshen.xdf.cn/v1";
const MODEL_NAME: &str = "gemini-3.0-pro-preview"; // 可以根据需要修改模型名称

/// LLM 请求配置
pub struct LlmConfig {
    /// API 密钥
    pub api_key: Option<String>,
    /// API 基础 URL
    pub api_base_url: Option<String>,
    /// 模型名称
    pub model_name: Option<String>,
    /// 系统消息
    pub system_message: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_base_url: None,
            model_name: None,
            system_message: None,
        }
    }
}

/// 通用的 LLM 调用函数（使用默认配置）
/// 
/// # 参数
/// - `user_message`: 用户消息内容
/// 
/// # 返回
/// 返回 LLM 的响应内容字符串
/// 
/// # 示例
/// ```no_run
/// use crate::ask_llm::ask_llm;
/// 
/// # async fn example() -> anyhow::Result<()> {
/// let response = ask_llm("你好，请介绍一下你自己").await?;
/// println!("{}", response);
/// # Ok(())
/// # }
/// ```
pub async fn ask_llm(user_message: &str) -> anyhow::Result<String> {
    ask_llm_with_config(user_message, None).await
}

/// 带自定义配置的 LLM 调用函数
/// 
/// # 参数
/// - `user_message`: 用户消息内容
/// - `config`: LLM 配置（可选，可以直接传 `LlmConfig` 或不传使用默认配置）
/// 
/// # 返回
/// 返回 LLM 的响应内容字符串
/// 
/// # 示例
/// ```no_run
/// use crate::ask_llm::{ask_llm_with_config, LlmConfig};
/// 
/// # async fn example() -> anyhow::Result<()> {
/// // 使用自定义配置
/// let config = LlmConfig {
///     system_message: Some("你是一个专业的助手".to_string()),
///     ..Default::default()
/// };
/// let response = ask_llm_with_config("你的问题", config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn ask_llm_with_config(
    user_message: &str,
    config: impl Into<Option<LlmConfig>>,
) -> anyhow::Result<String> {
    let config = config.into().unwrap_or_default();
    
    let api_key = config.api_key.as_deref().unwrap_or(API_KEY);
    let api_base_url = config.api_base_url.as_deref().unwrap_or(API_BASE_URL);
    let model_name = config.model_name.as_deref().unwrap_or(MODEL_NAME);
    
    debug!("正在调用 LLM API，模型: {}", model_name);
    debug!("用户消息: {}", user_message);
    
    let credentials = Credentials::new(api_key, api_base_url);
    
    let mut messages = Vec::new();
    
    // 添加系统消息（如果提供）
    if let Some(system_msg) = config.system_message {
        messages.push(ChatCompletionMessage {
            role: ChatCompletionMessageRole::System,
            content: Some(system_msg),
            name: None,
            function_call: None,
            tool_call_id: None,
            tool_calls: None,
        });
    }
    
    // 添加用户消息
    messages.push(ChatCompletionMessage {
        role: ChatCompletionMessageRole::User,
        content: Some(user_message.to_string()),
        name: None,
        function_call: None,
        tool_call_id: None,
        tool_calls: None,
    });
    
    let chat_completion = ChatCompletion::builder(model_name, messages)
        .credentials(credentials)
        .create()
        .await
        .map_err(|e| {
            warn!("LLM API 调用失败: {}", e);
            anyhow::anyhow!("LLM API 调用失败: {}", e)
        })?;
    
    debug!("LLM API 调用成功");
    
    let returned_message = chat_completion
        .choices
        .first()
        .ok_or_else(|| anyhow::anyhow!("LLM 返回结果为空"))?
        .message
        .clone();
    
    let content = returned_message
        .content
        .ok_or_else(|| anyhow::anyhow!("LLM 返回内容为空"))?;
    
    Ok(content.trim().to_string())
}

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
    debug!("候选城市列表: {:?}", matched_cities);
    
    let prompt = build_city_resolution_prompt(paper_name, province, matched_cities);
    debug!("LLM Prompt: {}", prompt);
    
    // 使用通用的 ask_llm_with_config 函数
    let config = LlmConfig {
        system_message: Some("你是一个专业的城市识别助手，能够根据试卷名称准确识别城市。".to_string()),
        ..Default::default()
    };
    
    let city_name = ask_llm_with_config(&prompt, config).await?;
    
    // 检查返回的城市是否在候选列表中
    if city_name == "无法确定" || city_name.is_empty() {
        info!("LLM 无法确定城市");
        return Ok(None);
    }
    
    // 检查返回的城市是否在候选列表中（支持带"市"或不带"市"）
    for matched_city in matched_cities {
        if city_name == *matched_city || city_name == matched_city.trim_end_matches("市") {
            info!("LLM 裁决结果: {}", matched_city);
            return Ok(Some(matched_city.clone()));
        }
    }
    
    // 如果返回的城市不在候选列表中，尝试直接匹配
    info!("LLM 返回的城市 '{}' 不在候选列表中，尝试直接使用", city_name);
    Ok(Some(city_name))
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