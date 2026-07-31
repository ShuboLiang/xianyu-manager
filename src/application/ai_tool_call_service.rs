//! 用例：AI 工具调用审计记录的查询。

use std::sync::Arc;

use crate::domain::ai_tool_call::AiToolCall;
use crate::domain::error::DomainError;
use crate::domain::repository::AiToolCallRepository;

pub struct AiToolCallService {
    calls: Arc<dyn AiToolCallRepository>,
}

impl AiToolCallService {
    pub fn new(calls: Arc<dyn AiToolCallRepository>) -> Self {
        Self { calls }
    }

    pub async fn list_recent(&self, limit: u32) -> Result<Vec<AiToolCall>, DomainError> {
        self.calls.list_recent(limit.clamp(1, 200)).await
    }
}
