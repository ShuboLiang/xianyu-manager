//! AI 用例：自动打标签（classify_service）与 AI 驱动抓取（crawl_agent_service）。
//!
//! 包含应用层 AiTool 实现：
//! - classify_service：`list_tags` / `apply_product_tags`
//! - crawl_agent_service：`xianyu_search` / `save_crawl_result`

pub mod classify_service;
pub mod crawl_agent_service;
