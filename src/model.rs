use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub origin: String,
    pub stem: String,
    #[serde(default)]
    pub origin_from_our_bank: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionPage {
    pub name: String,
    pub province: String,
    pub grade: String,
    #[serde(deserialize_with = "deserialize_year")]
    pub year: String,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_id: Option<String>,
    pub stemlist: Vec<Question>,
}

// Helper function to deserialize year as either string or integer
fn deserialize_year<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Visitor;
    use std::fmt;

    struct YearVisitor;

    impl<'de> Visitor<'de> for YearVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or integer representing a year")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }
    }

    deserializer.deserialize_any(YearVisitor)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutiThreadConfig {
    pub ports: Vec<i32>,
    pub zujvanwang_catalogue_url: String,
    #[serde(default)]
    pub zujvanwang_papers: Vec<PaperInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperInfo {
    pub url: String,
    pub title: String,
}

impl MutiThreadConfig {
    pub async fn create(
        ports: Vec<i32>,
        zujvanwang_catalogue_url: String,
        page: &chromiumoxide::Page,
    ) -> anyhow::Result<Self> {
        use tracing::warn;
        use serde_json::Value;

        let js_code = r#"
            () => {
                const elements = document.querySelectorAll("div.info-item.exam-info a.exam-name");
                return Array.from(elements).map(el => ({
                    url: 'https://zujuan.xkw.com' + el.getAttribute('href'),
                    title: el.innerText.trim()
                }));
            }
        "#;

        let response: Value = page
            .evaluate(js_code)
            .await?
            .into_value()?;

        let zujvanwang_papers: Vec<PaperInfo> = serde_json::from_value(response)?;

        if zujvanwang_papers.is_empty() {
            warn!("Warning: Could not find any question URLs on the catalogue page.");
        }

        Ok(Self {
            ports,
            zujvanwang_catalogue_url,
            zujvanwang_papers,
        })
    }
}
