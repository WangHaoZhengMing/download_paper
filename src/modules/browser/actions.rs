use anyhow::Result;
use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use std::path::Path;

/// 生成 PDF 文件
pub async fn generate_pdf(page: &chromiumoxide::Page, path: &str) -> Result<()> {
    let params = PrintToPdfParams::default();
    let pdf_path = Path::new(path);
    let _pdf_data = page.save_pdf(params, pdf_path).await?;
    Ok(())
}
