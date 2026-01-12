use tracing::warn;

use crate::modules::ask_llm;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct MiscInfo {
    pub paper_type_name: String,
    pub parent_paper_type: String,
    pub school_year_begin: Option<i32>,
    pub school_year_end: Option<i32>,
    pub paper_term: Option<String>,
    pub paper_month: Option<i32>,
}

impl Default for MiscInfo {
    fn default() -> Self {
        Self {
            paper_type_name: String::new(),
            parent_paper_type: String::new(),
            school_year_begin: Some(2024),
            school_year_end: Some(2025),
            paper_term: Some("1".to_string()),
            paper_month: None,
        }
    }
}

impl MiscInfo {
    pub fn prompt_for_llm( question_name: &str) -> String {
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
                - 如果标题中没有这个信息，返回 None

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
            question_name
        );
        user_message
    }

    pub async fn get_mis_info(question_name: &str) -> Option<MiscInfo> {

        let response = ask_llm(&Self::prompt_for_llm(question_name)).await.ok()?;
        match serde_json::from_str::<MiscInfo>(&response) {
            Ok(info) => Some(info),
            Err(e) => {
                warn!("解析 LLM 响应失败: {}，响应内容: {}", e, response);
                None
            }
        }
    }
}
