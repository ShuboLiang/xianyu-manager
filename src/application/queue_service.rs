//! 用例：抓取队列。详细设计见 docs/design-crawl-queue.md。
//!
//! - 入队目标二选一：选择器规则匹配 / 显式商品 id 列表；
//! - 全局去重 + 入队前预览；全局唯一 worker 串行执行，最多一个 running 队列；
//! - 优雅暂停（跑完当前条目再停）、间隔 sleep 切成 1 秒小片段；
//! - 服务启动时恢复：running 条目重置 pending、多余 running 队列降为 waiting。

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crate::application::ai::crawl_agent_service::CrawlAgentService;
use crate::application::ports::XianYuGateway;
use crate::domain::crawl_queue::{CrawlEntry, CrawlQueue, EntryStatus, QueueStatus, Selector};
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::product::Product;
use crate::domain::repository::{
    ItemRepository, ProductRepository, QueueRepository, TagRepository,
};

/// 入队目标：选择器或显式商品 id 列表，二选一
#[derive(Debug)]
pub enum EnqueueTarget {
    Selector(Selector),
    ProductIds(Vec<i64>),
}

/// 预览/入队结果：将新增与将被跳过（已在队列）的商品
#[derive(Debug)]
pub struct PreviewResult {
    pub to_add: Vec<Product>,
    pub skipped: Vec<Product>,
}

/// 队列 + 条目状态计数
#[derive(Debug)]
pub struct QueueProgress {
    pub queue: CrawlQueue,
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
}

pub struct QueueService {
    queues: Arc<dyn QueueRepository>,
    products: Arc<dyn ProductRepository>,
    tags: Arc<dyn TagRepository>,
    gateway: Arc<dyn XianYuGateway>,
    items: Arc<dyn ItemRepository>,
    /// AI 抓取（WebBridge 真实浏览器 + AI 筛选）；None 时走 gateway 直接抓取（mock 开发路径）
    crawl_agent: Option<Arc<CrawlAgentService>>,
}

impl QueueService {
    pub fn new(
        queues: Arc<dyn QueueRepository>,
        products: Arc<dyn ProductRepository>,
        tags: Arc<dyn TagRepository>,
        gateway: Arc<dyn XianYuGateway>,
        items: Arc<dyn ItemRepository>,
        crawl_agent: Option<Arc<CrawlAgentService>>,
    ) -> Self {
        Self {
            queues,
            products,
            tags,
            gateway,
            items,
            crawl_agent,
        }
    }

    // ---------- 查询 ----------

    pub async fn get_progress(&self, id: i64) -> Result<QueueProgress, DomainError> {
        let queue = self
            .queues
            .find_queue(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("队列 {id}")))?;
        self.progress_of(queue).await
    }

    pub async fn list_progress(&self) -> Result<Vec<QueueProgress>, DomainError> {
        let queues = self.queues.list_queues().await?;
        let mut result = Vec::with_capacity(queues.len());
        for q in queues {
            result.push(self.progress_of(q).await?);
        }
        Ok(result)
    }

    async fn progress_of(&self, queue: CrawlQueue) -> Result<QueueProgress, DomainError> {
        let entries = self.queues.list_entries(queue.id).await?;
        let count = |s: EntryStatus| entries.iter().filter(|e| e.status == s).count();
        Ok(QueueProgress {
            total: entries.len(),
            pending: count(EntryStatus::Pending),
            running: count(EntryStatus::Running),
            done: count(EntryStatus::Done),
            failed: count(EntryStatus::Failed),
            skipped: count(EntryStatus::Skipped),
            queue,
        })
    }

    // ---------- 入队 ----------

    pub async fn preview(&self, target: EnqueueTarget) -> Result<PreviewResult, DomainError> {
        let candidates = self.resolve_targets(&target).await?;
        let queued: HashSet<i64> = self.queues.queued_product_ids().await?.into_iter().collect();
        let (skipped, to_add): (Vec<_>, Vec<_>) =
            candidates.into_iter().partition(|p| queued.contains(&p.id));
        Ok(PreviewResult { to_add, skipped })
    }

    /// 创建队列并启动（或排队）。返回队列与入队明细。
    pub async fn enqueue(
        &self,
        target: EnqueueTarget,
        interval_secs: u32,
    ) -> Result<(CrawlQueue, PreviewResult), DomainError> {
        let preview = self.preview(target).await?;
        if preview.to_add.is_empty() {
            return Err(DomainError::InvalidInput(
                "没有可入队的商品（匹配为空或全部已在队列中）".into(),
            ));
        }
        let status = match self.queues.current_running_queue().await? {
            Some(_) => QueueStatus::Waiting,
            None => QueueStatus::Running,
        };
        let queue = self
            .queues
            .create_queue(&CrawlQueue::new(status, interval_secs.max(1)))
            .await?;
        let ids: Vec<i64> = preview.to_add.iter().map(|p| p.id).collect();
        self.queues.add_entries(queue.id, &ids).await?;
        tracing::info!("队列 #{} 已创建：{} 条，状态 {}", queue.id, ids.len(), status.as_str());
        Ok((queue, preview))
    }

    /// 向 waiting/running/paused 队列追加条目
    pub async fn append_entries(
        &self,
        queue_id: i64,
        target: EnqueueTarget,
    ) -> Result<PreviewResult, DomainError> {
        let queue = self
            .queues
            .find_queue(queue_id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("队列 {queue_id}")))?;
        match queue.status {
            QueueStatus::Waiting | QueueStatus::Running | QueueStatus::Paused => {}
            _ => {
                return Err(DomainError::InvalidState(format!(
                    "队列 {queue_id} 已结束，无法追加条目，请新建队列"
                )))
            }
        }
        let preview = self.preview(target).await?;
        let ids: Vec<i64> = preview.to_add.iter().map(|p| p.id).collect();
        self.queues.add_entries(queue_id, &ids).await?;
        Ok(preview)
    }

    /// 把入队目标解析成商品列表（去重前）
    async fn resolve_targets(&self, target: &EnqueueTarget) -> Result<Vec<Product>, DomainError> {
        match target {
            EnqueueTarget::ProductIds(ids) => {
                if ids.is_empty() {
                    return Err(DomainError::InvalidInput("商品 id 列表不能为空".into()));
                }
                let mut products = Vec::with_capacity(ids.len());
                for id in ids {
                    products.push(
                        self.products
                            .find(*id)
                            .await?
                            .ok_or_else(|| DomainError::NotFound(format!("商品 {id}")))?,
                    );
                }
                Ok(products)
            }
            EnqueueTarget::Selector(selector) => {
                selector.validate()?;
                // 选择器引用的标签必须存在
                for id in selector
                    .tag_all
                    .iter()
                    .chain(&selector.tag_any)
                    .chain(&selector.tag_exclude)
                {
                    if self.tags.find(*id).await?.is_none() {
                        return Err(DomainError::NotFound(format!("标签 {id}")));
                    }
                }
                let now = now_unix();
                let all = self.products.list().await?;
                Ok(all.into_iter().filter(|p| selector.matches(p, now)).collect())
            }
        }
    }

    // ---------- 状态操作 ----------

    pub async fn pause(&self, id: i64) -> Result<CrawlQueue, DomainError> {
        self.mutate_queue(id, |q| q.pause()).await
    }

    pub async fn resume(&self, id: i64) -> Result<CrawlQueue, DomainError> {
        self.mutate_queue(id, |q| q.resume()).await
    }

    pub async fn cancel(&self, id: i64) -> Result<CrawlQueue, DomainError> {
        self.mutate_queue(id, |q| q.cancel()).await
    }

    /// 全部暂停：running/waiting → paused，返回影响数量
    pub async fn pause_all(&self) -> Result<usize, DomainError> {
        let queues = self
            .queues
            .list_by_status(&[QueueStatus::Running, QueueStatus::Waiting])
            .await?;
        let count = queues.len();
        for mut q in queues {
            q.pause()?;
            self.queues.update_queue(&q).await?;
        }
        tracing::info!("全部暂停：{count} 个队列");
        Ok(count)
    }

    /// 全部恢复：paused → waiting，返回影响数量
    pub async fn resume_all(&self) -> Result<usize, DomainError> {
        let queues = self.queues.list_by_status(&[QueueStatus::Paused]).await?;
        let count = queues.len();
        for mut q in queues {
            q.resume()?;
            self.queues.update_queue(&q).await?;
        }
        tracing::info!("全部恢复：{count} 个队列");
        Ok(count)
    }

    /// 删除已结束（done/cancelled）的队列及其条目；活跃队列禁止删除
    pub async fn delete(&self, id: i64) -> Result<(), DomainError> {
        let queue = self
            .queues
            .find_queue(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("队列 {id}")))?;
        match queue.status {
            QueueStatus::Done | QueueStatus::Cancelled => {}
            _ => {
                return Err(DomainError::InvalidState(format!(
                    "队列 {id} 仍在活动（{}），不能删除；请先取消",
                    queue.status.as_str()
                )));
            }
        }
        self.queues.delete_queue(id).await?;
        tracing::info!("删除队列 {id}");
        Ok(())
    }

    async fn mutate_queue(
        &self,
        id: i64,
        f: impl FnOnce(&mut CrawlQueue) -> Result<(), DomainError>,
    ) -> Result<CrawlQueue, DomainError> {
        let mut queue = self
            .queues
            .find_queue(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("队列 {id}")))?;
        f(&mut queue)?;
        self.queues.update_queue(&queue).await?;
        Ok(queue)
    }

    // ---------- worker ----------

    /// 启动恢复 + 拉起全局 worker（服务启动时调用一次）
    pub async fn start_worker(self: &Arc<Self>) -> Result<(), DomainError> {
        // running 条目重置为 pending（进程死掉时请求肯定没完成）
        self.queues.reset_running_entries().await?;
        // running 队列最多保留最早的一个，其余降为 waiting
        let running = self.queues.list_by_status(&[QueueStatus::Running]).await?;
        for mut q in running.into_iter().skip(1) {
            q.status = QueueStatus::Waiting;
            self.queues.update_queue(&q).await?;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.worker_loop().await;
        });
        Ok(())
    }

    /// 全局唯一 worker：找 running 队列逐条处理；没有则提升最早的 waiting
    async fn worker_loop(&self) {
        loop {
            let running = self.queues.current_running_queue().await;
            match running {
                Ok(Some(queue)) => self.process_one_round(queue).await,
                Ok(None) => match self.queues.oldest_waiting_queue().await {
                    Ok(Some(mut w)) => {
                        w.status = QueueStatus::Running;
                        if let Err(e) = self.queues.update_queue(&w).await {
                            tracing::error!("提升队列 #{} 失败: {e}", w.id);
                        } else {
                            tracing::info!("队列 #{} 开始执行", w.id);
                        }
                    }
                    Ok(None) => tokio::time::sleep(Duration::from_secs(1)).await,
                    Err(e) => {
                        tracing::error!("worker 查询 waiting 队列失败: {e}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                },
                Err(e) => {
                    tracing::error!("worker 查询 running 队列失败: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// 处理当前 running 队列的一轮：一条条目 + 条间间隔；条目耗尽则队列完成
    async fn process_one_round(&self, queue: CrawlQueue) {
        match self.queues.next_pending_entry(queue.id).await {
            Ok(Some(entry)) => {
                self.process_entry(entry).await;
                self.sleep_interval(&queue).await;
            }
            Ok(None) => {
                let mut q = queue;
                q.finish();
                if let Err(e) = self.queues.update_queue(&q).await {
                    tracing::error!("完成队列 #{} 失败: {e}", q.id);
                } else {
                    tracing::info!("队列 #{} 已完成", q.id);
                }
            }
            Err(e) => {
                tracing::error!("worker 查询条目失败: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    /// 优雅暂停：当前条目跑完才停；条目只存 id，执行时现查商品
    async fn process_entry(&self, mut entry: CrawlEntry) {
        entry.status = EntryStatus::Running;
        let _ = self.queues.update_entry(&entry).await;

        let result = self.crawl_one(&entry).await;
        match result {
            Ok(()) => {
                entry.status = EntryStatus::Done;
                entry.crawled_at = Some(now_unix());
            }
            Err(EntryFailure::Skipped) => {
                entry.status = EntryStatus::Skipped;
            }
            Err(EntryFailure::Failed(msg)) => {
                entry.status = EntryStatus::Failed;
                entry.error = Some(msg);
            }
        }
        if let Err(e) = self.queues.update_entry(&entry).await {
            tracing::error!("更新条目 #{} 失败: {e}", entry.id);
        }
    }

    async fn crawl_one(&self, entry: &CrawlEntry) -> Result<(), EntryFailure> {
        let product = self
            .products
            .find(entry.product_id)
            .await
            .map_err(|e| EntryFailure::Failed(e.to_string()))?
            .ok_or(EntryFailure::Skipped)?;

        tracing::info!(
            "爬取商品: {} (id={}, queue={})",
            product.name.as_str(),
            product.id,
            entry.queue_id
        );

        // AI 抓取路径：WebBridge 搜索 → AI 筛选 8 条 → 工具内算中位数/回收价并落库
        if let Some(agent) = &self.crawl_agent {
            let outcome = agent
                .crawl_product(&product)
                .await
                .map_err(|e| EntryFailure::Failed(e.to_string()))?;
            tracing::info!(
                "商品 {} 抓取完成：{} 条有效，中位数 {:.2}，均价 {:.2}，回收价 {:.2}",
                product.id,
                outcome.count,
                outcome.median_price,
                outcome.avg_price,
                outcome.recycle_price
            );
            return Ok(());
        }

        let items = self
            .gateway
            .search(product.name.as_str(), 1)
            .await
            .map_err(|e| EntryFailure::Failed(e.to_string()))?;

        let _ = self.items.save_all(&items).await;

        let mut prices: Vec<f64> = items.iter().map(|i| i.price).collect();
        prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let count = prices.len();
        let (median, avg) = if count == 0 {
            (0.0, 0.0)
        } else {
            let median = if count % 2 == 1 {
                prices[count / 2]
            } else {
                (prices[count / 2 - 1] + prices[count / 2]) / 2.0
            };
            let avg = prices.iter().sum::<f64>() / count as f64;
            (median, avg)
        };

        // 回收价本期用均价占位（见方案 6 节）
        let mut product = product;
        product.record_crawl_result(median, avg, count as u32, avg);
        self.products
            .update(&product)
            .await
            .map_err(|e| EntryFailure::Failed(e.to_string()))?;
        Ok(())
    }

    /// 条间间隔：interval + 0..interval 抖动，切成 1 秒小片段以便暂停/取消 1 秒内生效
    async fn sleep_interval(&self, queue: &CrawlQueue) {
        let jitter = pseudo_rand(queue.interval_secs);
        let total = queue.interval_secs + jitter;
        for _ in 0..total {
            tokio::time::sleep(Duration::from_secs(1)).await;
            // 队列不再是 running（被暂停/取消）则提前结束睡眠
            match self.queues.find_queue(queue.id).await {
                Ok(Some(q)) if q.status == QueueStatus::Running => {}
                _ => return,
            }
        }
    }
}

/// 条目级失败：Skipped（商品已删除）与 Failed（执行出错）分开
enum EntryFailure {
    Skipped,
    Failed(String),
}

/// 简易伪随机（避免引入 rand crate）：取当前时间纳秒取模
fn pseudo_rand(max_inclusive: u32) -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as u64 % (max_inclusive as u64 + 1)) as u32
}
