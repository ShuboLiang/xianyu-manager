//! 组装入口：config → infrastructure → application → interfaces

mod application;
mod domain;
mod infrastructure;
mod interfaces;

use std::sync::Arc;

use application::ai::classify_service::ClassifyService;
use application::ai_provider_service::AiProviderService;
use application::ai_tool_call_service::AiToolCallService;
use application::crawl_service::CrawlService;
use application::item_service::ItemService;
use application::ports::XianYuGateway;
use application::product_service::ProductService;
use application::queue_service::QueueService;
use application::stats_service::StatsService;
use application::tag_service::TagService;
use infrastructure::config::Config;
use infrastructure::persistence::memory::{
    InMemoryAiClassifyTaskRepository, InMemoryCrawlTaskRepository, InMemoryItemRepository,
};
use infrastructure::persistence::sqlite::{
    self, SqliteAiProviderRepository, SqliteAiToolCallRepository, SqliteProductRepository,
    SqliteQueueRepository, SqliteTagRepository,
};
use infrastructure::xianyu_gateway::{HttpXianYuGateway, MockXianYuGateway};
use interfaces::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xianyu_manager=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env();

    // infrastructure：仓储与网关实现
    let item_repo = Arc::new(InMemoryItemRepository::default());
    let task_repo = Arc::new(InMemoryCrawlTaskRepository::default());
    let pool = sqlite::connect(&config.database_path).await?;
    let tag_repo = Arc::new(SqliteTagRepository::new(pool.clone()));
    let product_repo = Arc::new(SqliteProductRepository::new(pool.clone()));
    let queue_repo = Arc::new(SqliteQueueRepository::new(pool.clone()));
    let ai_provider_repo = Arc::new(SqliteAiProviderRepository::new(pool.clone()));
    let ai_tool_call_repo = Arc::new(SqliteAiToolCallRepository::new(pool));
    let gateway: Arc<dyn XianYuGateway> = match config.gateway.as_str() {
        "http" => Arc::new(HttpXianYuGateway::new(std::env::var("XIANYU_COOKIE").ok())),
        _ => Arc::new(MockXianYuGateway),
    };

    // application：用例服务
    let crawl_service = Arc::new(CrawlService::new(gateway.clone(), item_repo.clone(), task_repo));
    let item_service = Arc::new(ItemService::new(item_repo.clone()));
    let tag_service = Arc::new(TagService::new(tag_repo.clone()));
    let product_service = Arc::new(ProductService::new(product_repo.clone(), tag_repo.clone()));

    let ai_gateway: Arc<dyn crate::application::ports::AiGateway> = Arc::new(
        crate::infrastructure::ai_gateway::RigAiGateway::new(
            ai_provider_repo.clone(),
            ai_tool_call_repo.clone(),
            crate::application::ports::AiEnvFallback {
                api_key: config.ai_fallback.api_key.clone(),
                base_url: config.ai_fallback.base_url.clone(),
                model: config.ai_fallback.model.clone(),
            },
        ),
    );
    let ai_env_fallback = crate::application::ports::AiEnvFallback {
        api_key: config.ai_fallback.api_key.clone(),
        base_url: config.ai_fallback.base_url.clone(),
        model: config.ai_fallback.model.clone(),
    };
    let ai_provider_service = Arc::new(AiProviderService::new(
        ai_provider_repo,
        ai_gateway.clone(),
        ai_env_fallback,
    ));
    let ai_tool_call_service = Arc::new(AiToolCallService::new(ai_tool_call_repo));

    let classify_task_repo = Arc::new(InMemoryAiClassifyTaskRepository::default());
    let classify_service = Arc::new(ClassifyService::new(
        classify_task_repo,
        product_repo.clone(),
        tag_repo.clone(),
        ai_gateway,
    ));

    let queue_service = Arc::new(QueueService::new(
        queue_repo,
        product_repo.clone(),
        tag_repo,
        gateway,
        item_repo.clone(),
    ));

    let stats_service = Arc::new(StatsService::new(item_repo, product_repo));

    // 启动恢复 + 拉起全局抓取 worker
    queue_service.start_worker().await?;

    // interfaces：HTTP 路由
    let app = interfaces::build_router(
        AppState {
            crawl_service,
            item_service,
            tag_service,
            product_service,
            queue_service,
            ai_provider_service,
            ai_tool_call_service,
            classify_service,
            stats_service,
        },
        &config.static_dir,
    );

    let addr = config.listen_addr();
    tracing::info!("服务已启动: http://{addr} (gateway={})", config.gateway);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
