/// 试卷处理结果
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProcessResult {
    Success,
    AlreadyExists,
    Failed,
}

