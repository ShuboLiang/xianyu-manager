//! 组装入口：config → infrastructure → application → interfaces

mod application;
mod domain;
mod infrastructure;
mod interfaces;

use std::sync::Arc;

use application::ai::admin_tools::AdminToolsService;
use application::ai::chat_session_service::ChatSessionService;
use application::ai::classify_service::ClassifyService;
use application::ai::crawl_agent_service::CrawlAgentService;
use application::ai::crawl_direct_service::CrawlDirectService;
use application::ai::crawl_switch::SwitchableCrawler;
use application::ai::tool_approval::ToolApprovalRegistry;
use application::ai_provider_service::AiProviderService;
use application::ai_settings_service::AiSettingsService;
use application::ai_tool_call_service::AiToolCallService;
use application::crawl_service::CrawlService;
use application::item_service::ItemService;
use application::ports::XianYuGateway;
use application::product_service::ProductService;
use application::queue_service::QueueService;
use application::stats_service::StatsService;
use application::tag_service::TagService;
use application::trend_service::TrendService;
use infrastructure::config::Config;
use infrastructure::persistence::memory::{
    InMemoryAiClassifyTaskRepository, InMemoryCrawlTaskRepository,
};
use infrastructure::persistence::sqlite::{
    self, SqliteAiProviderRepository, SqliteAiToolCallRepository, SqliteConversationRepository,
    SqliteItemRepository, SqliteProductRepository, SqliteQueueRepository,
    SqliteSettingsRepository, SqliteTagRepository,
};
use infrastructure::webbridge_client::{launch_webbridge_daemon, WebBridgeClient};
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
    let task_repo = Arc::new(InMemoryCrawlTaskRepository::default());
    let pool = sqlite::connect(&config.database_path).await?;
    let item_repo = Arc::new(SqliteItemRepository::new(pool.clone()));
    let tag_repo = Arc::new(SqliteTagRepository::new(pool.clone()));
    let product_repo = Arc::new(SqliteProductRepository::new(pool.clone()));
    let queue_repo = Arc::new(SqliteQueueRepository::new(pool.clone()));
    let ai_provider_repo = Arc::new(SqliteAiProviderRepository::new(pool.clone()));
    let ai_tool_call_repo = Arc::new(SqliteAiToolCallRepository::new(pool.clone()));
    let conversation_repo = Arc::new(SqliteConversationRepository::new(pool.clone()));
    let settings_repo = Arc::new(SqliteSettingsRepository::new(pool));
    // GATEWAY=webbridge：真实抓取走 WebBridge 浏览器（同时作为普通网关供 CrawlService 取原始候选）
    if config.gateway == "webbridge" {
        if let Some(bin_path) = &config.webbridge_bin_path {
            if let Err(e) = launch_webbridge_daemon(bin_path, &config.webbridge_url).await {
                tracing::warn!("自动启动 WebBridge 失败: {e}");
            }
        } else {
            tracing::warn!("未配置 WEBBRIDGE_BIN_PATH，无法自动启动 WebBridge");
        }
    }
    let webbridge = (config.gateway == "webbridge")
        .then(|| Arc::new(WebBridgeClient::new(&config.webbridge_url)));
    let gateway: Arc<dyn XianYuGateway> = match (config.gateway.as_str(), webbridge.clone()) {
        ("webbridge", Some(client)) => client,
        ("http", _) => Arc::new(HttpXianYuGateway::new(std::env::var("XIANYU_COOKIE").ok())),
        _ => Arc::new(MockXianYuGateway),
    };

    // application：用例服务
    let crawl_service = Arc::new(CrawlService::new(gateway.clone(), item_repo.clone(), task_repo));
    let item_service = Arc::new(ItemService::new(item_repo.clone()));
    let tag_service = Arc::new(TagService::new(tag_repo.clone()));
    let product_service = Arc::new(ProductService::new(
        product_repo.clone(),
        tag_repo.clone(),
        item_repo.clone(),
        queue_repo.clone(),
    ));
    let trend_service = Arc::new(TrendService::new(item_repo.clone(), product_repo.clone()));

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
    let ai_tool_call_service = Arc::new(AiToolCallService::new(ai_tool_call_repo.clone()));

    let classify_task_repo = Arc::new(InMemoryAiClassifyTaskRepository::default());
    let classify_service = Arc::new(ClassifyService::new(
        classify_task_repo,
        product_repo.clone(),
        tag_repo.clone(),
        ai_gateway.clone(),
    ));

    // GATEWAY=webbridge 时，队列条目由 AI 抓取（搜索 → 筛选 → 统计落库）。
    // 两种实现同时构造，SwitchableCrawler 每次抓取读 app_settings 的 ai_crawl_mode
    // （DB 设置 > AI_CRAWL_MODE 环境变量兜底）决定走哪条，前端切换后下一轮生效。
    let ai_gateway_for_admin = ai_gateway.clone();
    let crawler: Option<Arc<dyn crate::application::ai::crawl_shared::ProductCrawler>> =
        webbridge.map(|client| {
            let direct = Arc::new(CrawlDirectService::new(
                client.clone(),
                product_repo.clone(),
                item_repo.clone(),
                ai_gateway.clone(),
                settings_repo.clone(),
                tag_repo.clone(),
                ai_tool_call_repo.clone(),
                config.recycle_factor,
            ));
            let agent = Arc::new(CrawlAgentService::new(
                client,
                product_repo.clone(),
                item_repo.clone(),
                ai_gateway,
                settings_repo.clone(),
                tag_repo.clone(),
                config.recycle_factor,
            ));
            Arc::new(SwitchableCrawler::new(
                direct,
                agent,
                settings_repo.clone(),
                config.ai_crawl_mode.clone(),
            )) as Arc<dyn crate::application::ai::crawl_shared::ProductCrawler>
        });
    let ai_settings_service = Arc::new(AiSettingsService::new(
        settings_repo,
        config.ai_crawl_mode.clone(),
    ));

    let queue_service = Arc::new(QueueService::new(
        queue_repo,
        product_repo.clone(),
        tag_repo,
        gateway,
        item_repo.clone(),
        crawler,
    ));

    let stats_service = Arc::new(StatsService::new(item_repo, product_repo));

    // 写操作确认闸口：AI 助手会话的 yolo/normal 模式与待确认审批（进程内存态）
    let tool_approval = ToolApprovalRegistry::new();

    let admin_tools_service = Arc::new(AdminToolsService::new(
        product_service.clone(),
        tag_service.clone(),
        item_service.clone(),
        queue_service.clone(),
        stats_service.clone(),
        trend_service.clone(),
        ai_gateway_for_admin,
        tool_approval.clone(),
    ));
    let chat_session_service = Arc::new(ChatSessionService::new(
        conversation_repo,
        admin_tools_service.clone(),
        tool_approval.clone(),
    ));

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
            ai_settings_service,
            ai_tool_call_service,
            classify_service,
            admin_tools_service,
            chat_session_service,
            tool_approval: Arc::new(tool_approval),
            stats_service,
            trend_service,
        },
        &config.static_dir,
    );

    let addr = config.listen_addr();
    tracing::info!("服务已启动: http://{addr} (gateway={})", config.gateway);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
