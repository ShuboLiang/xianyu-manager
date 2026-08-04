//! 抓取任务实体：承载任务状态流转的领域规则。

use super::error::DomainError;
use super::item::{Keyword, PageRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
}

/// 抓取任务（实体）。状态流转只能通过实体方法进行：
/// 只有 Pending 能 start，只有 Running 能 finish/fail。
#[derive(Debug, Clone)]
pub struct CrawlTask {
    pub id: String,
    pub keyword: Keyword,
    pub max_pages: PageRange,
    pub status: TaskStatus,
    pub item_count: usize,
    pub error: Option<String>,
    /// Unix 秒
    pub created_at: u64,
}

impl CrawlTask {
    pub fn new(keyword: Keyword, max_pages: PageRange) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            keyword,
            max_pages,
            status: TaskStatus::Pending,
            item_count: 0,
            error: None,
            created_at: now_unix(),
        }
    }

    pub fn start(&mut self) -> Result<(), DomainError> {
        if self.status != TaskStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "任务 {} 当前状态不是 Pending，无法启动",
                self.id
            )));
        }
        self.status = TaskStatus::Running;
        Ok(())
    }

    pub fn finish(&mut self, item_count: usize) -> Result<(), DomainError> {
        if self.status != TaskStatus::Running {
            return Err(DomainError::InvalidState(format!(
                "任务 {} 当前状态不是 Running，无法完成",
                self.id
            )));
        }
        self.item_count = item_count;
        self.status = TaskStatus::Done;
        Ok(())
    }

    /// 只有 Running 能标记失败（与 finish 同一约束）：
    /// 失败是「执行中出了错」，不允许覆盖已结束的任务状态
    pub fn fail(&mut self, message: impl Into<String>) -> Result<(), DomainError> {
        if self.status != TaskStatus::Running {
            return Err(DomainError::InvalidState(format!(
                "任务 {} 当前状态不是 Running，无法标记失败",
                self.id
            )));
        }
        self.error = Some(message.into());
        self.status = TaskStatus::Failed;
        Ok(())
    }
}

/// 当前 Unix 秒（避免引入 chrono，骨架阶段够用）
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
