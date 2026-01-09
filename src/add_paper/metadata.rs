use crate::add_paper::models::MiscByAi;
use crate::add_paper::utils::clean_json_string;
use crate::ask_llm::{ask_llm, resolve_city_with_llm};
use crate::bank_page_info::address::{get_city_code, match_cities_from_paper_name};
use crate::bank_page_info::grade::find_grade_code;
use crate::bank_page_info::paper_type::PaperCategory;
use crate::bank_page_info::subject::find_subject_code;
use crate::model::QuestionPage;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use tracing::{debug, error, info, warn};

/// 试卷元数据构建器
pub struct MetadataBuilder;

impl MetadataBuilder {
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

    /// 构建试卷保存的 payload
    pub async fn build_paper_payload(
        question_page: &QuestionPage,
        attachments: Option<Value>,
    ) -> Result<Value> {
        // 确定城市
        debug!("开始确定城市信息");
        let city_code = Self::determine_city_from_paper_name(&question_page.name, &question_page.province)
            .await
            .map_err(|e| {
                error!("确定城市失败: {}", e);
                e
            })?;
        debug!("城市 code: {:?}", city_code);

        debug!("构建试卷保存 payload");
        // 先计算附件数量（在使用 attachments 之前）
        let attachment_count = if let Some(att) = &attachments {
            if let Some(arr) = att.as_array() {
                arr.len()
            } else {
                0
            }
        } else {
            0
        };
        debug!("附件数量: {}", attachment_count);

        let user_message = format!(
            r#"你是一个专业的教务数据分析助手。请根据试卷名称 "{}" 分析并提取元数据。

请严格遵守以下规则，返回一个纯 JSON 对象，不要包含 markdown 格式标记（如 ```json ... ```）。只用给我返回Json 对象就可以了！！！
请严格遵守以下规则，返回一个纯 JSON 对象，不要包含 markdown 格式标记（如 ```json ... ```）。只用给我返回Json 对象就可以了！！！
请严格遵守以下规则，返回一个纯 JSON 对象，不要包含 markdown 格式标记（如 ```json ... ```）。只用给我返回Json 对象就可以了！！！
### 字段定义与约束：

1. **paper_type_name** (String): 试卷类型。必须从以下列表中选择最合适的一个：
   - 中考真题, 中考模拟, 学业考试, 自主招生
   - 小初衔接, 初高衔接
   - 期中考试, 期末考试, 单元测试, 开学考试, 月考, 周测, 课堂闭环, 阶段测试
   - 教材, 教辅
   - 竞赛

2. **parent_paper_type** (String): 试卷大类。请根据你选择的 `paper_type_name`，按照以下映射关系自动填充：
   - 若类型为 [中考真题, 中考模拟, 学业考试, 自主招生] -> 归类为 "中考专题"
   - 若类型为 [小初衔接, 初高衔接] -> 归类为 "跨学段衔接"
   - 若类型为 [期中考试, 期末考试, 单元测试, 开学考试, 月考, 周测, 课堂闭环, 阶段测试] -> 归类为 "阶段测试"
   - 若类型为 [教材, 教辅] -> 归类为 "新东方自研"
   - 若类型为 [竞赛] -> 归类为 "竞赛"

3. **school_year_begin** (i32): 学年开始年份。例如 2023。
4. **school_year_end** (i32): 学年结束年份。例如 2024。
   - 逻辑参考：
     - 2023-2024 -> begin=2023, end=2024
     - 2024年下学期(春季) -> 属于 2023-2024 学年 -> begin=2023, end=2024
     - 2024年上学期(秋季) -> 属于 2024-2025 学年 -> begin=2024, end=2025

5. **paper_term** (String): 学期。**注意：必须返回字符串类型的数字**。
   - "1" 代表上学期（秋季）
   - "2" 代表下学期（春季）


6. **paper_month** (Integer): 考试月份。**注意：必须返回整数**。
   - 如果标题中没有这个信息，返回 None

### JSON 返回示例：
{{
  "paper_type_name": "期中考试",
  "parent_paper_type": "阶段测试",
  "school_year_begin": 2023,
  "school_year_end": 2024,
  "paper_term": "2", 
  "paper_month": 4
}}
"#,
            question_page.name
        );

        let llm_json_response = ask_llm(&user_message).await?;
        let cleaned_response = clean_json_string(&llm_json_response);
        let parsed_data: MiscByAi = serde_json::from_str(cleaned_response)
            .with_context(|| format!("LLM 返回的 JSON 解析失败，原始内容：{}", llm_json_response))?;
        debug!(
            "解析成功：\n试卷类型：{} \n 试卷parent:{} \n学年：{}-{}\n学期：{:?}\n月份：{:?}",
            parsed_data.paper_type_name,
            parsed_data.parent_paper_type,
            parsed_data.school_year_begin,
            parsed_data.school_year_end,
            parsed_data.paper_term,
            parsed_data.paper_month
        );

        let mut payload = json!({
            "paperType":crate::bank_page_info::paper_type::get_subtype_value_by_name(&question_page.subject,&parsed_data.paper_type_name),
            "parentPaperType": PaperCategory::get_value(&parsed_data.parent_paper_type).unwrap_or_else(||{warn!("Not found parentPaperType, using default"); "ppt1"}),
            "schName": "集团",
            "schNumber": "65",

            "schoolYearBegin": parsed_data.school_year_begin,
            "schoolYearEnd": parsed_data.school_year_end,
            "paperTerm": parsed_data.paper_term.unwrap_or_else(||{warn!("not found paper_term, using \"\" by default");"".to_string()}),
            "paperYear": question_page.year.parse::<i32>().unwrap_or_else(|_|{warn!("Can not parse year, using 2024 by default"); 2024}),
            "courseVersionCode": "",
            "address": [
            {
                "province": crate::bank_page_info::address::get_province_code(&question_page.province).unwrap_or_else(||{warn!("Can not get province code, using 1 by default");1}).to_string(),
                "city": city_code.unwrap_or(0).to_string() // 如果无法确定城市，使用 0
            }
            ],
            "title": &question_page.name,
            "stage": "3",
            "stageName": "初中",
            "subject": find_subject_code(&question_page.subject).unwrap().to_string(),
            "subjectName": &question_page.subject,
            "gradeName": &question_page.grade,
            "grade": find_grade_code(&question_page.grade).unwrap_or_else(||{warn!("Can not infer grade or find. Using 161 default"); 161}).to_string(),

            "paperId": "",
            "attachments": attachments.unwrap_or_else(|| json!([]))
        });

        if let Some(month) = parsed_data.paper_month {
            payload["paperMonth"] = json!(month);
        }
        debug!("Payload 构建完成;");

        Ok(payload)
    }
}

