//! AI 自动打标签服务：同步路径（≤50 商品）+ 异步任务路径（>50）。
//!
//! 包含两个应用层 AiTool 实现：
//! - `list_tags`：实时查库返回全部 enabled=true 的标签
//! - `apply_product_tags`：批量写入标签关联

pub mod classify_service;
