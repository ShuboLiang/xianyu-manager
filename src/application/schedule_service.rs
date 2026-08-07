//! 定时抓取用例：到点时仅创建现有抓取队列，实际抓取仍完全由 QueueService 的单 worker 完成。

use std::sync::Arc;
use std::time::Duration;

use crate::domain::crawl_queue::QueueStatus;
use crate::domain::crawl_schedule::{CrawlSchedule, NewCrawlSchedule, ScheduleName};
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::repository::{ScheduleRepository, TagRepository};

use super::queue_service::{EnqueueTarget, QueueService};
use crate::domain::crawl_queue::Selector;

#[derive(Debug, Clone, Default)]
pub struct UpdateSchedule {
    pub name: Option<String>,
    pub tag_ids: Option<Vec<i64>>,
    pub every_days: Option<u32>,
    pub queue_interval_secs: Option<u32>,
    pub enabled: Option<bool>,
    pub next_run_at: Option<u64>,
}

pub struct ScheduleService {
    schedules: Arc<dyn ScheduleRepository>,
    tags: Arc<dyn TagRepository>,
    queues: Arc<QueueService>,
}

impl ScheduleService {
    pub fn new(
        schedules: Arc<dyn ScheduleRepository>,
        tags: Arc<dyn TagRepository>,
        queues: Arc<QueueService>,
    ) -> Self {
        Self { schedules, tags, queues }
    }

    pub async fn list(&self) -> Result<Vec<CrawlSchedule>, DomainError> {
        self.schedules.list().await
    }

    pub async fn get(&self, id: i64) -> Result<CrawlSchedule, DomainError> {
        self.schedules
            .find(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("定时任务 {id}")))
    }

    pub async fn create(&self, mut input: NewCrawlSchedule) -> Result<CrawlSchedule, DomainError> {
        input.tag_ids = crate::domain::crawl_schedule::normalized_tags(input.tag_ids);
        input.validate()?;
        self.ensure_enabled_tags(&input.tag_ids).await?;
        self.schedules.create(&input).await
    }

    pub async fn update(&self, id: i64, input: UpdateSchedule) -> Result<CrawlSchedule, DomainError> {
        let mut schedule = self.get(id).await?;
        if let Some(tag_ids) = &input.tag_ids {
            self.ensure_enabled_tags(tag_ids).await?;
        }
        let name = input.name.map(ScheduleName::new).transpose()?;
        schedule.update(
            name,
            input.tag_ids,
            input.every_days,
            input.queue_interval_secs,
            input.enabled,
            input.next_run_at,
        )?;
        self.schedules.update(&schedule).await?;
        Ok(schedule)
    }

    pub async fn delete(&self, id: i64) -> Result<(), DomainError> {
        if !self.schedules.delete(id).await? {
            return Err(DomainError::NotFound(format!("定时任务 {id}")));
        }
        Ok(())
    }

    /// 用户“立即执行”不改原本的周期锚点，避免手动补抓后把每周计划整体推迟。
    pub async fn run_now(&self, id: i64) -> Result<CrawlSchedule, DomainError> {
        let schedule = self.get(id).await?;
        if !schedule.enabled {
            return Err(DomainError::InvalidState("定时任务已暂停，恢复后才能立即执行".into()));
        }
        self.execute(schedule, false).await
    }

    /// 启动常驻调度器；一分钟粒度足以满足“每 N 天”的第一版语义。
    pub fn start_worker(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                if let Err(e) = this.process_due().await {
                    tracing::error!("定时任务调度失败: {e}");
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    }

    async fn process_due(&self) -> Result<(), DomainError> {
        let now = now_unix();
        for schedule in self.schedules.list().await? {
            if schedule.enabled && schedule.next_run_at <= now {
                if let Err(e) = self.execute(schedule, true).await {
                    tracing::error!("执行定时任务失败: {e}");
                }
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        mut schedule: CrawlSchedule,
        advance_schedule: bool,
    ) -> Result<CrawlSchedule, DomainError> {
        let now = now_unix();

        // 同一任务上次生成的队列尚未结束时不再叠加队列，防止慢任务越积越多。
        if let Some(queue_id) = schedule.last_queue_id {
            if let Ok(progress) = self.queues.get_progress(queue_id).await {
                if matches!(progress.queue.status, QueueStatus::Waiting | QueueStatus::Running | QueueStatus::Paused) {
                    schedule.mark_run(now, None, "上次队列尚未结束，本次已跳过".into());
                    if advance_schedule {
                        schedule.advance_to_next_slot(now);
                    }
                    self.schedules.update(&schedule).await?;
                    return Ok(schedule);
                }
            }
        }

        if let Err(e) = self.ensure_enabled_tags(&schedule.tag_ids).await {
            schedule.disable_with_message(format!("已自动暂停：{e}"));
            self.schedules.update(&schedule).await?;
            return Ok(schedule);
        }

        let target = EnqueueTarget::Selector(Selector {
            tag_any: schedule.tag_ids.clone(),
            ..Default::default()
        });
        match self.queues.preview(target.clone()).await {
            Ok(preview) if preview.to_add.is_empty() => {
                schedule.mark_run(
                    now,
                    None,
                    format!("无可入队商品（{} 个已在活跃队列）", preview.skipped.len()),
                );
            }
            Ok(_) => match self.queues.enqueue(target, schedule.queue_interval_secs).await {
                Ok((queue, result)) => {
                    schedule.mark_run(
                        now,
                        Some(queue.id),
                        format!("已创建队列 #{}：新增 {} 个，跳过 {} 个", queue.id, result.to_add.len(), result.skipped.len()),
                    );
                }
                Err(e) => schedule.mark_run(now, None, format!("入队失败：{e}")),
            },
            Err(e) => schedule.mark_run(now, None, format!("预览失败：{e}")),
        }
        if advance_schedule {
            schedule.advance_to_next_slot(now);
        }
        self.schedules.update(&schedule).await?;
        Ok(schedule)
    }

    async fn ensure_enabled_tags(&self, tag_ids: &[i64]) -> Result<(), DomainError> {
        if tag_ids.is_empty() {
            return Err(DomainError::InvalidInput("定时任务没有可用标签".into()));
        }
        for id in tag_ids {
            match self.tags.find(*id).await? {
                None => return Err(DomainError::NotFound(format!("标签 {id}"))),
                Some(tag) if !tag.enabled => {
                    return Err(DomainError::InvalidInput(format!("标签「{}」已停用", tag.name.as_str())))
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}
