//! AI 用例：自动打标签（classify_service）与 AI 驱动抓取（两种实现可配置切换）。
//!
//! 包含应用层 AiTool 实现：
//! - classify_service：`list_tags` / `apply_product_tags`
//! - crawl_agent_service：`xianyu_search` / `save_crawl_result`（ReAct 路径，AI_CRAWL_MODE=agent）
//! - crawl_direct_service：单轮调用路径（AI_CRAWL_MODE=direct，默认，省 token）
//! - crawl_shared：两种路径共享的 ProductCrawler 端口、CrawlOutcome 与统计落库逻辑

pub mod admin_tools;
pub mod chat_session_service;
pub mod classify_service;
pub mod crawl_agent_service;
pub mod crawl_direct_service;
pub mod crawl_shared;
pub mod crawl_switch;
