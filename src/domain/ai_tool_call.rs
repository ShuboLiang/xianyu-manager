//! AI 工具调用审计实体：run_agent 桥接层对每次工具执行落一条记录，
//! 成功记 result、失败记 error，供事后回查。

/// 审计记录来源（写入时标记该次调用由哪个用例产生，前端「调用记录」按来源筛选）
pub mod source {
    /// AI 助手（通用管理 agent / 会话系统）
    pub const ASSISTANT: &str = "assistant";
    /// AI 驱动抓取（direct 与 agent 两种模式）
    pub const CRAWL: &str = "crawl";
    /// AI 自动打标签（同步/异步）
    pub const CLASSIFY: &str = "classify";
}

/// 创建审计记录的入参（尚无 id）
#[derive(Debug, Clone)]
pub struct NewAiToolCall {
    pub tool_name: String,
    /// 调用参数（JSON 字符串）
    pub arguments: String,
    /// 成功结果（JSON 字符串）
    pub result: Option<String>,
    /// 失败错误信息
    pub error: Option<String>,
    pub duration_ms: u64,
    /// LLM 调用的输入 token 数（纯工具行为 None；供应商未上报也为 None）
    pub input_tokens: Option<u64>,
    /// LLM 调用的输出 token 数
    pub output_tokens: Option<u64>,
    /// 命中供应商缓存的输入 token 数
    pub cached_input_tokens: Option<u64>,
    /// 来源（source 模块常量）：assistant / crawl / classify
    pub source: String,
}

/// AI 工具调用记录（实体，只增不改）
#[derive(Debug, Clone)]
pub struct AiToolCall {
    pub id: i64,
    pub tool_name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    /// Unix 秒
    pub created_at: u64,
    /// 来源；老库迁移前的记录为 None
    pub source: Option<String>,
}
