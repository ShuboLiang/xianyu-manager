//! 定时抓取任务：独立于标签存在，标签仅是任务的圈选条件。

use super::crawl_task::now_unix;
use super::error::DomainError;

pub const MAX_SCHEDULE_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleName(String);

impl ScheduleName {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into().trim().to_string();
        if value.is_empty() {
            return Err(DomainError::InvalidInput("定时任务名称不能为空".into()));
        }
        if value.chars().count() > MAX_SCHEDULE_NAME_LEN {
            return Err(DomainError::InvalidInput(format!(
                "定时任务名称过长（>{MAX_SCHEDULE_NAME_LEN} 字符）"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct NewCrawlSchedule {
    pub name: ScheduleName,
    pub tag_ids: Vec<i64>,
    /// 每 N 天执行一次。第一版刻意只支持固定天数周期，避免 cron 配置难以排查。
    pub every_days: u32,
    /// 创建出来的普通抓取队列内部的条目间隔。
    pub queue_interval_secs: u32,
    pub first_run_at: u64,
}

#[derive(Debug, Clone)]
pub struct CrawlSchedule {
    pub id: i64,
    pub name: ScheduleName,
    pub tag_ids: Vec<i64>,
    pub every_days: u32,
    pub queue_interval_secs: u32,
    pub enabled: bool,
    pub next_run_at: u64,
    pub last_run_at: Option<u64>,
    pub last_queue_id: Option<i64>,
    pub last_message: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl NewCrawlSchedule {
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_settings(&self.tag_ids, self.every_days, self.queue_interval_secs)
    }
}

impl CrawlSchedule {
    pub fn update(
        &mut self,
        name: Option<ScheduleName>,
        tag_ids: Option<Vec<i64>>,
        every_days: Option<u32>,
        queue_interval_secs: Option<u32>,
        enabled: Option<bool>,
        next_run_at: Option<u64>,
    ) -> Result<(), DomainError> {
        let new_tags = tag_ids.unwrap_or_else(|| self.tag_ids.clone());
        let new_days = every_days.unwrap_or(self.every_days);
        let new_interval = queue_interval_secs.unwrap_or(self.queue_interval_secs);
        validate_settings(&new_tags, new_days, new_interval)?;
        if let Some(name) = name {
            self.name = name;
        }
        self.tag_ids = normalized_tags(new_tags);
        self.every_days = new_days;
        self.queue_interval_secs = new_interval;
        if let Some(enabled) = enabled {
            self.enabled = enabled;
        }
        if let Some(next_run_at) = next_run_at {
            self.next_run_at = next_run_at;
        }
        self.touch();
        Ok(())
    }

    pub fn mark_run(&mut self, now: u64, queue_id: Option<i64>, message: String) {
        self.last_run_at = Some(now);
        if queue_id.is_some() {
            self.last_queue_id = queue_id;
        }
        self.last_message = Some(message);
        self.touch();
    }

    /// 按原本的时间轴推进，服务停机后只合并成下一次未来执行点，不补跑积压周期。
    pub fn advance_to_next_slot(&mut self, now: u64) {
        let interval = self.every_days as u64 * 86_400;
        let mut next = self.next_run_at.saturating_add(interval);
        while next <= now {
            next = next.saturating_add(interval);
        }
        self.next_run_at = next;
        self.touch();
    }

    pub fn disable_with_message(&mut self, message: String) {
        self.enabled = false;
        self.last_message = Some(message);
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = now_unix();
    }
}

pub fn normalized_tags(mut tag_ids: Vec<i64>) -> Vec<i64> {
    tag_ids.sort_unstable();
    tag_ids.dedup();
    tag_ids
}

fn validate_settings(tag_ids: &[i64], every_days: u32, queue_interval_secs: u32) -> Result<(), DomainError> {
    if tag_ids.is_empty() || tag_ids.iter().any(|id| *id <= 0) {
        return Err(DomainError::InvalidInput("定时任务至少要选择一个有效标签".into()));
    }
    if !(1..=365).contains(&every_days) {
        return Err(DomainError::InvalidInput("执行周期必须在 1 至 365 天之间".into()));
    }
    if !(1..=3_600).contains(&queue_interval_secs) {
        return Err(DomainError::InvalidInput("抓取间隔必须在 1 至 3600 秒之间".into()));
    }
    Ok(())
}
