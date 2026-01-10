use anyhow::{Ok, Result};
use chromiumoxide::Page;
use serde_json::{Value, json};
use std::path::Path;
use tracing::warn;
use tracing::{debug, error, info};

use crate::core::QuestionPage;
use crate::modules::browser::upload_pdf_to_server;
use crate::modules::build_save_paper_js;
use crate::modules::metadata::data_addr::get_province_code;
use crate::modules::metadata::data_grade::find_grade_code;
use crate::modules::metadata::data_paper_type::{PaperCategory, get_subtype_value_by_name};
use crate::modules::metadata::data_subject::find_subject_code;
use crate::modules::metadata::deter_city::determine_city_from_paper_name;
use crate::modules::metadata::deter_misc::MiscInfo;

pub async fn save_paper(tiku_page: Page, _question_page: &mut QuestionPage) -> anyhow::Result<()> {
    let playload = construct_upload_payload(_question_page, tiku_page.clone()).await?;
    debug!("上传试卷负载: {}", playload);
    let code = build_save_paper_js(&playload);
    debug!("保存试卷的 JS 代码: {}", code);
    let response: chromiumoxide::js::EvaluationResult = tiku_page.evaluate(code).await?;
    debug!("保存试卷响应: {:?}", response);
    // 2. 转为通用的 JSON Value
    let json_val: Value = response.into_value()?;

    // 3. 提取 data 字段
    // 注意：as_str() 返回 Option<&str>，需要处理 None 的情况
    if let Some(data_str) = json_val.get("data").and_then(|v| v.as_str()) {
        let paper_id = data_str.to_string();
        info!("Paper ID: {}", paper_id);
        _question_page.set_page_id(paper_id);
    } else {
        error!("无法找到 data 字段或 data 不是字符串");
    }
    
    Ok(())
}

async fn construct_upload_payload(question_page: &QuestionPage, tiku_page: Page) -> Result<String> {
    let city_code = determine_city_from_paper_name(&question_page.name, &question_page.province)
        .await?
        .unwrap_or(0)
        .to_string();
    let province_code = get_province_code(&question_page.province).unwrap_or(0);
    let parsed_data = MiscInfo::get_mis_info(&question_page.name).await.unwrap();

    let attachments = upload_pdf_to_server(
        &tiku_page,
        Path::new(&format!("PDF/{}.pdf", question_page.name_for_pdf)),
    )
    .await?;

    let payload = json!({
        "paperType":get_subtype_value_by_name(&question_page.subject,&parsed_data.paper_type_name),

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
            "province": province_code.to_string(),
            "city": city_code
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

    Ok(payload.to_string())
}
