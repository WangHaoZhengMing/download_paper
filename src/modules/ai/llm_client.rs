use openai::chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole};
use openai::Credentials;
use tracing::{debug, info, warn};

const API_KEY: &str = "26e96c4d312e48feacbd78b7c42bd71e";
const API_BASE_URL: &str = "http://menshen.xdf.cn/v1";
const MODEL_NAME: &str = "gemini-3.0-pro-preview";

pub struct LlmConfig {
    pub api_key: Option<String>,
    pub api_base_url: Option<String>,
    pub model_name: Option<String>,
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

pub async fn ask_llm(user_message: &str) -> anyhow::Result<String> {
    ask_llm_with_config(user_message, None).await
}

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
        "请根据试卷名称判断应该选择哪个城市。\n\n试卷名称：{}\n{}匹配到的候选城市（{}个）：\n{}\n\n请只返回一个最匹配的城市名称，不要包含其他内容。如果无法确定，请返回\"无法确定\"。",
        paper_name,
        province_info,
        matched_cities.len(),
        cities_list
    )
}

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

    let config = LlmConfig {
        system_message: Some("你是一个专业的城市识别助手，能够根据试卷名称准确识别城市。".to_string()),
        ..Default::default()
    };

    let city_name = ask_llm_with_config(&prompt, config).await?;

    if city_name == "无法确定" || city_name.is_empty() {
        info!("LLM 无法确定城市");
        return Ok(None);
    }

    for matched_city in matched_cities {
        if city_name == *matched_city || city_name == matched_city.trim_end_matches("市") {
            info!("LLM 裁决结果: {}", matched_city);
            return Ok(Some(matched_city.clone()));
        }
    }

    info!("LLM 返回的城市 '{}' 不在候选列表中，尝试直接使用", city_name);
    Ok(Some(city_name))
}

