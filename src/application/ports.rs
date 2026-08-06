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

/// 一次 LLM 调用的 token 用量（供应商未上报时整个为 None）。
/// cached_input_tokens 为命中供应商前缀缓存的输入 token 数，可用于观察缓存是否生效。
#[derive(Debug, Clone, Copy)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

/// 单次对话补全的结果：文本 + token 用量
#[derive(Debug, Clone)]
pub struct AiCompletion {
    pub text: String,
    pub usage: Option<TokenUsage>,
}

/// AI 网关：抽象「对话补全」与「带工具的 agent 循环」两档能力。
/// 实现方负责 HTTP 调用、鉴权、重试、工具循环等易变细节；
/// rig 等第三方库类型不允许泄漏到该 trait 之外。
#[allow(dead_code)]
#[async_trait]
pub trait AiGateway: Send + Sync {
    /// 检查 AI 是否可用（已配置 DB 默认 provider 或环境变量兜底）
    async fn is_available(&self) -> bool;

    /// 档位 1：单次对话补全（返回文本与 token 用量，供调用方落审计）
    async fn complete(&self, system: &str, user: &str) -> Result<AiCompletion, DomainError>;

    /// 档位 2：带工具的 agent 循环（ReAct）。tools 由应用层定义，
    /// 实现方负责「模型请求工具 → 执行 → 结果回填 → 再调用」的循环，
    /// 直到模型给出最终答案；max_rounds 封顶防工具调用死循环。
    /// source 标记本次 agent 归属（ai_tool_call::source 常量），写入审计记录。
    /// approval 为写工具确认闸口：Some 时写工具（is_write()=true）先经人工确认再执行；
    /// None（抓取/打标签等无人值守流程）时写工具直接执行，不弹确认。
    async fn run_agent(
        &self,
        system: &str,
        user: &str,
        tools: &[Arc<dyn AiTool>],
        max_rounds: u32,
        source: &str,
        approval: Option<Arc<dyn ToolApproval>>,
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
    /// 是否为写操作（会真实改库）。默认 false；写工具实现覆盖为 true，
    /// AI 助手在「正常模式」下执行写工具前会先征求用户同意。
    fn is_write(&self) -> bool {
        false
    }
}

/// 写工具执行前的人工确认闸口（应用层定义，infra 的 run_agent 在执行写工具前调用）。
/// 每个会话一个实例：yolo 模式自动放行；「允许本次」放行一次；「该对话全部允许」放行该工具；
/// 其余情况创建待确认审批并阻塞等待用户决策，直到前端提交决策（或超时自动拒绝）。
#[async_trait]
pub trait ToolApproval: Send + Sync {
    /// 返回 true=放行执行，false=拒绝（拒绝结果作为工具错误回填给模型）
    async fn check(&self, tool_name: &str, arguments: &str) -> Result<bool, DomainError>;
}
