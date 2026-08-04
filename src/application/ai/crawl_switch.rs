//! 运行时切换的抓取器：包装 direct / agent 两种实现，
//! 每次抓取从 app_settings 读取当前模式（DB 设置 > AI_CRAWL_MODE 环境变量兜底），
//! 前端在「AI 配置」页切换后下一轮抓取即生效，无需重启。

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ai::crawl_agent_service::CrawlAgentService;
use crate::application::ai::crawl_direct_service::CrawlDirectService;
use crate::application::ai::crawl_shared::{CrawlOutcome, ProductCrawler};
use crate::application::ai_settings_service::{CRAWL_MODE_AGENT, CRAWL_MODE_KEY};
use crate::domain::error::DomainError;
use crate::domain::product::Product;
use crate::domain::repository::SettingsRepository;

pub struct SwitchableCrawler {
    direct: Arc<CrawlDirectService>,
    agent: Arc<CrawlAgentService>,
    settings: Arc<dyn SettingsRepository>,
    env_mode: String,
}

impl SwitchableCrawler {
    pub fn new(
        direct: Arc<CrawlDirectService>,
        agent: Arc<CrawlAgentService>,
        settings: Arc<dyn SettingsRepository>,
        env_mode: String,
    ) -> Self {
        Self {
            direct,
            agent,
            settings,
            env_mode,
        }
    }

    /// 当前生效的模式（读设置失败时兜底 direct 并记警告，不阻塞抓取）
    async fn current_mode(&self) -> String {
        match self.settings.get(CRAWL_MODE_KEY).await {
            Ok(Some(v)) if !v.trim().is_empty() => v,
            Ok(_) => self.env_mode.clone(),
            Err(e) => {
                tracing::warn!("读取抓取模式失败（{e}），本轮用 direct");
                "direct".into()
            }
        }
    }
}

#[async_trait]
impl ProductCrawler for SwitchableCrawler {
    async fn check_ai_available(&self) -> bool {
        self.direct.check_ai_available().await
    }

    async fn crawl_product(&self, product: &Product) -> Result<CrawlOutcome, DomainError> {
        let mode = self.current_mode().await;
        if mode == CRAWL_MODE_AGENT {
            tracing::debug!("商品 {} 走 agent（ReAct）抓取路径", product.id);
            self.agent.crawl_product(product).await
        } else {
            tracing::debug!("商品 {} 走 direct（单轮调用）抓取路径", product.id);
            self.direct.crawl_product(product).await
        }
    }
}
