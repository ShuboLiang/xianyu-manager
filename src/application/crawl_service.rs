//! 用例：发起抓取（异步任务）、查询任务状态。

use std::sync::Arc;

use crate::domain::crawl_task::CrawlTask;
use crate::domain::error::DomainError;
use crate::domain::item::{Keyword, PageRange};
use crate::domain::repository::{CrawlTaskRepository, ItemRepository};

use super::ports::XianYuGateway;

pub struct CrawlService {
    gateway: Arc<dyn XianYuGateway>,
    items: Arc<dyn ItemRepository>,
    tasks: Arc<dyn CrawlTaskRepository>,
}

impl CrawlService {
    pub fn new(
        gateway: Arc<dyn XianYuGateway>,
        items: Arc<dyn ItemRepository>,
        tasks: Arc<dyn CrawlTaskRepository>,
    ) -> Self {
        Self {
            gateway,
            items,
            tasks,
        }
    }

    /// 创建抓取任务并派发到后台执行，立即返回任务句柄。
    pub async fn start_crawl(
        self: &Arc<Self>,
        keyword: String,
        max_pages: u32,
    ) -> Result<CrawlTask, DomainError> {
        let task = CrawlTask::new(Keyword::new(keyword)?, PageRange::new(max_pages)?);
        self.tasks.save(&task).await?;

        let this = Arc::clone(self);
        let task_id = task.id.clone();
        tokio::spawn(async move {
            if let Err(e) = this.run_task(&task_id).await {
                tracing::error!("抓取任务 {task_id} 失败: {e}");
            }
        });

        Ok(task)
    }

    pub async fn get_task(&self, id: &str) -> Result<CrawlTask, DomainError> {
        self.tasks
            .find(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("任务 {id}")))
    }

    /// 后台执行：启动 → 逐页抓取 → 落库 → 完成/失败
    async fn run_task(&self, task_id: &str) -> Result<(), DomainError> {
        let mut task = self.get_task(task_id).await?;
        task.start()?;
        self.tasks.save(&task).await?;

        let result = self.fetch_pages(&task).await;
        match result {
            Ok(count) => task.finish(count)?,
            Err(e) => task.fail(e.to_string()),
        }
        self.tasks.save(&task).await
    }

    async fn fetch_pages(&self, task: &CrawlTask) -> Result<usize, DomainError> {
        let mut total = 0;
        for page in 1..=task.max_pages.value() {
            let batch = self.gateway.search(task.keyword.as_str(), page).await?;
            total += batch.len();
            self.items.save_all(&batch).await?;
        }
        Ok(total)
    }
}
