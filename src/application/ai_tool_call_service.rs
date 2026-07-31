//! 用例：AI 工具调用审计记录的查询（分页）。

use std::sync::Arc;

use crate::domain::ai_tool_call::AiToolCall;
use crate::domain::error::DomainError;
use crate::domain::repository::{AiToolCallRepository, Page};

pub struct AiToolCallService {
    calls: Arc<dyn AiToolCallRepository>,
}

impl AiToolCallService {
    pub fn new(calls: Arc<dyn AiToolCallRepository>) -> Self {
        Self { calls }
    }

    /// 按时间倒序分页（page 从 1 开始，调用方已完成钳制）
    pub async fn list_paginated(
        &self,
        page: u64,
        page_size: u64,
    ) -> Result<Page<AiToolCall>, DomainError> {
        self.calls
            .list_paginated((page - 1) * page_size, page_size)
            .await
    }
}
