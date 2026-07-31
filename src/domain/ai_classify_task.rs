//! AI 自动打标签任务实体：承载异步任务状态流转的领域规则。
//! 内存仓储，重启即失，同 CrawlTask 模式。

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use super::crawl_task::now_unix;
use super::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ClassifyTaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl ClassifyTaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// AI 分类建议：工具实际写入结果中单条商品的打标签信息
#[derive(Debug, Clone)]
pub struct ClassifyWarning {
    pub product_id: i64,
    pub message: String,
}

/// AI 自动打标签任务（实体）。状态流转只能通过实体方法进行：
/// 只有 Pending 能 start，只有 Running 能 finish/fail/cancel。
#[derive(Debug, Clone)]
pub struct AiClassifyTask {
    pub id: String,
    pub status: ClassifyTaskStatus,
    /// 待处理的全部商品 id
    pub product_ids: Vec<i64>,
    /// 批次大小
    pub batch_size: usize,
    /// 总商品数
    pub total: usize,
    /// 已处理商品数
    pub processed: usize,
    /// 成功打标签的商品数
    pub succeeded: usize,
    /// 失败的商品数
    pub failed: usize,
    /// 汇总警告
    pub warnings: Vec<ClassifyWarning>,
    /// 当前处理到第几批（从 0 开始）
    pub current_batch: usize,
    /// 错误信息（Failed 时有值）
    pub error: Option<String>,
    /// Unix 秒
    pub created_at: u64,
    pub finished_at: Option<u64>,
}

impl AiClassifyTask {
    pub fn new(product_ids: Vec<i64>, batch_size: usize) -> Self {
        let total = product_ids.len();
        Self {
            id: short_uuid(),
            status: ClassifyTaskStatus::Pending,
            product_ids,
            batch_size,
            total,
            processed: 0,
            succeeded: 0,
            failed: 0,
            warnings: Vec::new(),
            current_batch: 0,
            error: None,
            created_at: now_unix(),
            finished_at: None,
        }
    }

    /// 下一次批次的切片返回；返回 None 表示全部处理完毕
    pub fn next_batch(&mut self) -> Option<Vec<i64>> {
        let start = self.current_batch * self.batch_size;
        if start >= self.total {
            return None;
        }
        let end = (start + self.batch_size).min(self.total);
        self.current_batch += 1;
        Some(self.product_ids[start..end].to_vec())
    }

    pub fn start(&mut self) -> Result<(), DomainError> {
        if self.status != ClassifyTaskStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "分类任务 {} 当前状态不是 Pending，无法启动",
                self.id
            )));
        }
        self.status = ClassifyTaskStatus::Running;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), DomainError> {
        if self.status != ClassifyTaskStatus::Running {
            return Err(DomainError::InvalidState(format!(
                "分类任务 {} 当前状态不是 Running，无法完成",
                self.id
            )));
        }
        self.status = ClassifyTaskStatus::Done;
        self.finished_at = Some(now_unix());
        Ok(())
    }

    #[allow(dead_code)]
    pub fn fail(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.status = ClassifyTaskStatus::Failed;
        self.finished_at = Some(now_unix());
    }

    pub fn cancel(&mut self) -> Result<(), DomainError> {
        if self.status != ClassifyTaskStatus::Running {
            return Err(DomainError::InvalidState(format!(
                "分类任务 {} 当前状态不是 Running，无法取消",
                self.id
            )));
        }
        self.status = ClassifyTaskStatus::Cancelled;
        self.finished_at = Some(now_unix());
        Ok(())
    }

    /// 记录一批处理结果
    pub fn record_batch(&mut self, batch_succeeded: usize, batch_failed: usize, warnings: Vec<ClassifyWarning>) {
        self.processed += batch_succeeded + batch_failed;
        self.succeeded += batch_succeeded;
        self.failed += batch_failed;
        self.warnings.extend(warnings);
    }

    /// 进度百分比
    pub fn progress_pct(&self) -> f64 {
        if self.total == 0 {
            return 100.0;
        }
        (self.processed as f64 / self.total as f64) * 100.0
    }
}

fn short_uuid() -> String {
    let h1 = RandomState::new().build_hasher().finish();
    let h2 = RandomState::new().build_hasher().finish();
    format!("{h1:016x}{h2:016x}")
}
