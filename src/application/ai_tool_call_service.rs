//! 用例：AI 工具调用审计记录的查询（分页 + 筛选）与保留期清理。

use std::sync::Arc;

use crate::domain::ai_tool_call::AiToolCall;
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::repository::{AiToolCallRepository, Page};

pub struct AiToolCallService {
    calls: Arc<dyn AiToolCallRepository>,
}

/// 清理条件（二选一，构造时校验）：删除 N 天前的记录 / 仅保留最新 N 条
#[derive(Debug, Clone, Copy)]
pub enum PurgeCriteria {
    BeforeDays(u32),
    KeepLatest(u64),
}

impl PurgeCriteria {
    /// before_days 与 keep_latest 恰有一个为 Some，否则报错
    pub fn new(before_days: Option<u32>, keep_latest: Option<u64>) -> Result<Self, DomainError> {
        match (before_days, keep_latest) {
            (Some(d), None) => Ok(Self::BeforeDays(d)),
            (None, Some(n)) => Ok(Self::KeepLatest(n)),
            _ => Err(DomainError::InvalidInput(
                "清理条件二选一：before_days（删除 N 天前）或 keep_latest（仅保留最新 N 条）".into(),
            )),
        }
    }

    /// 转成仓储层的 (before_ts, keep_latest) 形式
    fn as_repo_args(&self) -> (Option<u64>, Option<u64>) {
        match self {
            Self::BeforeDays(d) => {
                let before_ts = now_unix().saturating_sub(*d as u64 * 86400);
                (Some(before_ts), None)
            }
            Self::KeepLatest(n) => (None, Some(*n)),
        }
    }
}

impl AiToolCallService {
    pub fn new(calls: Arc<dyn AiToolCallRepository>) -> Self {
        Self { calls }
    }

    /// 按时间倒序分页（page 从 1 开始，调用方已完成钳制）；
    /// tool_name 精确匹配筛选，failed_only=true 只看失败
    pub async fn list_paginated(
        &self,
        page: u64,
        page_size: u64,
        tool_name: Option<&str>,
        failed_only: Option<bool>,
    ) -> Result<Page<AiToolCall>, DomainError> {
        self.calls
            .list_paginated((page - 1) * page_size, page_size, tool_name, failed_only)
            .await
    }

    /// 库中出现过的工具名（筛选下拉用）
    pub async fn list_tool_names(&self) -> Result<Vec<String>, DomainError> {
        self.calls.list_tool_names().await
    }

    /// 预览将清理的记录数
    pub async fn purge_preview(&self, criteria: PurgeCriteria) -> Result<u64, DomainError> {
        let (before_ts, keep_latest) = criteria.as_repo_args();
        self.calls.purge_preview(before_ts, keep_latest).await
    }

    /// 执行清理，返回实际删除条数
    pub async fn purge(&self, criteria: PurgeCriteria) -> Result<u64, DomainError> {
        let (before_ts, keep_latest) = criteria.as_repo_args();
        let deleted = self.calls.purge(before_ts, keep_latest).await?;
        tracing::info!("AI 工具调用记录清理完成：删除 {deleted} 条（条件：{criteria:?}）");
        Ok(deleted)
    }
}
