//! 应用层的端口（port）：闲鱼数据网关、AI 网关。
//! 由 application 定义契约，infrastructure 提供实现（防腐层）。

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::ai_provider::AiProvider;
use crate::domain::error::DomainError;
use crate::domain::item::Item;

/// 闲鱼数据网关：抽象「按关键词搜索一页商品」的能力。
/// 实现方负责登录态、签名、HTML/JSON 解析等易变细节。
#[async_trait]
pub trait XianYuGateway: Send + Sync {
    async fn search(&self, keyword: &str, page: u32) -> Result<Vec<Item>, DomainError>;
}

/// 环境变量兜底配置（优先级：数据库默认配置 > 环境变量）
#[derive(Debug, Clone)]
pub struct AiEnvFallback {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}

/// AI 网关：抽象「对话补全」与「带工具的 agent 循环」两档能力。
/// 实现方负责 HTTP 调用、鉴权、重试、工具循环等易变细节；
/// rig 等第三方库类型不允许泄漏到该 trait 之外。
#[allow(dead_code)]
#[async_trait]
pub trait AiGateway: Send + Sync {
    /// 检查 AI 是否可用（已配置 DB 默认 provider 或环境变量兜底）
    async fn is_available(&self) -> bool;

    /// 档位 1：单次对话补全
    async fn complete(&self, system: &str, user: &str) -> Result<String, DomainError>;

    /// 档位 2：带工具的 agent 循环（ReAct）。tools 由应用层定义，
    /// 实现方负责「模型请求工具 → 执行 → 结果回填 → 再调用」的循环，
    /// 直到模型给出最终答案；max_rounds 封顶防工具调用死循环。
    async fn run_agent(
        &self,
        system: &str,
        user: &str,
        tools: &[Arc<dyn AiTool>],
        max_rounds: u32,
    ) -> Result<String, DomainError>;

    /// 用指定配置做连通性测试（不经过默认配置解析）
    async fn complete_with(
        &self,
        provider: &AiProvider,
        system: &str,
        user: &str,
    ) -> Result<String, DomainError>;
}

/// 应用层定义的工具端口：名称/参数 schema/执行逻辑都在这里，
/// infrastructure 只负责把它翻译成模型厂商的 function/tool 规格。
/// AI 工具完全自动执行（无人工确认），所有调用由实现方落审计日志。
#[async_trait]
pub trait AiTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// 参数的 JSON Schema
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value, DomainError>;
}
