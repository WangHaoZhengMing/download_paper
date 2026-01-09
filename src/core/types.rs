use crate::core::models::QuestionPage;

#[derive(Debug, Clone)]
pub struct EnrichedPaper {
    pub page: QuestionPage,
    pub pdf_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessResult {
    Success,
    AlreadyExists,
    Failed,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct ProcessStats {
    pub success: usize,
    pub exists: usize,
    pub failed: usize,
}

impl ProcessStats {
    pub fn add_result(&mut self, result: &ProcessResult) {
        match result {
            ProcessResult::Success => self.success += 1,
            ProcessResult::AlreadyExists => self.exists += 1,
            ProcessResult::Failed => self.failed += 1,
        }
    }
}
