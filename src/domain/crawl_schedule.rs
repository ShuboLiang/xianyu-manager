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
    /// 当前仍未结束的队列；存在时本任务不会重复入队。
    pub active_queue_id: Option<i64>,
    /// 当前队列结束后是否要以结束时间为起点重新计时（立即执行为 false）。
    pub active_queue_affects_schedule: bool,
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

    pub fn mark_check(&mut self, now: u64, message: String, advance_schedule: bool) {
        self.last_run_at = Some(now);
        self.last_message = Some(message);
        if advance_schedule {
            self.schedule_after(now);
        }
        self.touch();
    }

    /// 队列已创建后不再推进 next_run_at；必须等队列到终态才开始下一周期。
    pub fn mark_queue_started(
        &mut self,
        now: u64,
        queue_id: i64,
        affects_schedule: bool,
        message: String,
    ) {
        self.last_run_at = Some(now);
        self.last_queue_id = Some(queue_id);
        self.active_queue_id = Some(queue_id);
        self.active_queue_affects_schedule = affects_schedule;
        self.last_message = Some(message);
        self.touch();
    }

    /// 当前队列结束：定时触发的队列以结束时间为起点重新计时；立即执行不改原计划。
    pub fn complete_active_queue(&mut self, finished_at: u64) {
        let affects_schedule = self.active_queue_affects_schedule;
        self.active_queue_id = None;
        self.active_queue_affects_schedule = false;
        if affects_schedule {
            self.schedule_after(finished_at);
            self.last_message = Some("本轮队列已结束，已从完成时间开始计算下一周期".into());
        } else {
            self.last_message = Some("立即执行队列已结束，原定时计划未改变".into());
        }
        self.touch();
    }

    /// 服务重启兼容：旧版本已创建但尚未结束的队列在启动时重新认领。
    pub fn restore_active_queue(&mut self, queue_id: i64) {
        self.active_queue_id = Some(queue_id);
        self.active_queue_affects_schedule = true;
        self.touch();
    }

    /// 关联队列被历史清理而无法得知结束时间时，从当前时刻重新开始周期，避免立即重复入队。
    pub fn lose_active_queue(&mut self, now: u64) {
        let affects_schedule = self.active_queue_affects_schedule;
        self.active_queue_id = None;
        self.active_queue_affects_schedule = false;
        if affects_schedule {
            self.schedule_after(now);
            self.last_message = Some("关联队列记录已清理，已从当前时间重新计算下一周期".into());
        }
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

    fn schedule_after(&mut self, base: u64) {
        self.next_run_at = base.saturating_add(self.every_days as u64 * 86_400);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_queue_waits_until_completion_before_starting_next_cycle() {
        let mut schedule = CrawlSchedule {
            id: 1,
            name: ScheduleName::new("测试").unwrap(),
            tag_ids: vec![1],
            every_days: 7,
            queue_interval_secs: 3,
            enabled: true,
            next_run_at: 100,
            last_run_at: None,
            last_queue_id: None,
            active_queue_id: None,
            active_queue_affects_schedule: false,
            last_message: None,
            created_at: 0,
            updated_at: 0,
        };
        schedule.mark_queue_started(100, 9, true, "已入队".into());
        assert_eq!(schedule.next_run_at, 100);
        schedule.complete_active_queue(160);
        assert_eq!(schedule.next_run_at, 160 + 7 * 86_400);
    }
}
