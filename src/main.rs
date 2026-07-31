//! 组装入口：config → infrastructure → application → interfaces

mod application;
mod domain;
mod infrastructure;
mod interfaces;

use std::sync::Arc;

use application::crawl_service::CrawlService;
use application::item_service::ItemService;
use application::ports::XianYuGateway;
use infrastructure::config::Config;
use infrastructure::persistence::memory::{InMemoryCrawlTaskRepository, InMemoryItemRepository};
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
    let gateway: Arc<dyn XianYuGateway> = match config.gateway.as_str() {
        "http" => Arc::new(HttpXianYuGateway::new(std::env::var("XIANYU_COOKIE").ok())),
        _ => Arc::new(MockXianYuGateway),
    };

    // application：用例服务
    let crawl_service = Arc::new(CrawlService::new(gateway, item_repo.clone(), task_repo));
    let item_service = Arc::new(ItemService::new(item_repo));

    // interfaces：HTTP 路由
    let app = interfaces::build_router(
        AppState {
            crawl_service,
            item_service,
        },
        &config.static_dir,
    );

    let addr = config.listen_addr();
    tracing::info!("服务已启动: http://{addr} (gateway={})", config.gateway);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
