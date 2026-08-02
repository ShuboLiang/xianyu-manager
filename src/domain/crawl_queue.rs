//! 抓取队列：选择器（入队规则）、队列与队列条目实体。
//! 详细设计见 docs/design-crawl-queue.md。
//!
//! 设计要点：
//! - 选择器只在入队那一刻求值，求值结果是**商品 id 快照**（CrawlEntry），
//!   之后队列与标签无关——删除标签不影响已在队列的商品；
//! - 串行 worker 逐条处理，条目只存 id，执行时现查商品：
//!   商品被删 → 条目标记 Skipped；商品改名 → 用最新名称搜索；
//! - 全局最多一个 running 队列；提升只发生在 worker 循环边界。

use super::crawl_task::now_unix;
use super::error::DomainError;
use super::product::Product;

/// 入队选择器：各维度之间是 AND；标签内部支持「全部包含 / 任一包含 / 排除」
#[derive(Debug, Clone, Default)]
pub struct Selector {
    /// 必须全部包含的标签
    pub tag_all: Vec<i64>,
    /// 至少包含其一的标签
    pub tag_any: Vec<i64>,
    /// 不能包含的标签
    pub tag_exclude: Vec<i64>,
    /// 最后爬取时间距今 ≥ N 天（从未爬过的商品也算）；None = 不做时间过滤
    pub stale_days: Option<u32>,
    /// 只匹配无标签的商品
    pub no_tag: bool,
}

impl Selector {
    /// 至少需要一个圈选条件，防止误操作全量入队
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.tag_all.is_empty()
            && self.tag_any.is_empty()
            && self.tag_exclude.is_empty()
            && self.stale_days.is_none()
            && !self.no_tag
        {
            return Err(DomainError::InvalidInput(
                "选择器不能为空：至少选择一个标签条件或时间条件".into(),
            ));
        }
        Ok(())
    }

    pub fn matches(&self, product: &Product, now: u64) -> bool {
        if self.no_tag && !product.tag_ids.is_empty() {
            return false;
        }
        if !self.tag_all.iter().all(|t| product.tag_ids.contains(t)) {
            return false;
        }
        if !self.tag_any.is_empty() && !self.tag_any.iter().any(|t| product.tag_ids.contains(t)) {
            return false;
        }
        if self.tag_exclude.iter().any(|t| product.tag_ids.contains(t)) {
            return false;
        }
        if let Some(days) = self.stale_days {
            let threshold = now.saturating_sub(days as u64 * 86400);
            // 近期爬过的（在阈值之后）不匹配；从未爬过（None）匹配
            if let Some(t) = product.last_crawled_at {
                if t > threshold {
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueStatus {
    /// 排队中：等待执行位空闲
    Waiting,
    Running,
    Paused,
    Done,
    Cancelled,
}

impl QueueStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, DomainError> {
        match s {
            "waiting" => Ok(Self::Waiting),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "done" => Ok(Self::Done),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(DomainError::Infrastructure(format!("非法队列状态: {s}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

impl EntryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, DomainError> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            _ => Err(DomainError::Infrastructure(format!("非法条目状态: {s}"))),
        }
    }
}

/// 抓取队列（实体）。状态流转只能通过实体方法进行。
#[derive(Debug, Clone)]
pub struct CrawlQueue {
    pub id: i64,
    pub status: QueueStatus,
    /// 每条抓取之间的基础间隔（秒），执行时叠加随机抖动
    pub interval_secs: u32,
    pub created_at: u64,
    pub finished_at: Option<u64>,
}

impl CrawlQueue {
    /// 新队列：执行位空闲则 Running，否则 Waiting 排队
    pub fn new(status: QueueStatus, interval_secs: u32) -> Self {
        Self {
            id: 0,
            status,
            interval_secs,
            created_at: now_unix(),
            finished_at: None,
        }
    }

    /// 暂停：Running 或 Waiting 都可暂停（全部暂停是批量调用此方法）
    pub fn pause(&mut self) -> Result<(), DomainError> {
        match self.status {
            QueueStatus::Running | QueueStatus::Waiting => {
                self.status = QueueStatus::Paused;
                Ok(())
            }
            _ => Err(DomainError::InvalidState(
                "只有运行中或排队中的队列能暂停".into(),
            )),
        }
    }

    /// 恢复：转为 Waiting 排队（执行位空闲时 worker 会在下一轮自动提升）
    pub fn resume(&mut self) -> Result<(), DomainError> {
        if self.status != QueueStatus::Paused {
            return Err(DomainError::InvalidState("只有已暂停的队列能恢复".into()));
        }
        self.status = QueueStatus::Waiting;
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), DomainError> {
        match self.status {
            QueueStatus::Waiting | QueueStatus::Running | QueueStatus::Paused => {
                self.status = QueueStatus::Cancelled;
                self.finished_at = Some(now_unix());
                Ok(())
            }
            _ => Err(DomainError::InvalidState("已结束的队列不能取消".into())),
        }
    }

    /// worker 在条目耗尽时调用
    pub fn finish(&mut self) {
        self.status = QueueStatus::Done;
        self.finished_at = Some(now_unix());
    }
}

/// 队列条目（实体）：商品 id 快照，不带外键，商品删除后条目保留。
#[derive(Debug, Clone)]
pub struct CrawlEntry {
    pub id: i64,
    pub queue_id: i64,
    pub product_id: i64,
    pub status: EntryStatus,
    pub error: Option<String>,
    pub crawled_at: Option<u64>,
}

impl CrawlEntry {
    /// 构造新条目（当前条目由 SQL 直接插入，保留此方法备用）
    #[allow(dead_code)]
    pub fn new(queue_id: i64, product_id: i64) -> Self {
        Self {
            id: 0,
            queue_id,
            product_id,
            status: EntryStatus::Pending,
            error: None,
            crawled_at: None,
        }
    }
}


