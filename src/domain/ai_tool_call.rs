//! AI 工具调用审计实体：run_agent 桥接层对每次工具执行落一条记录，
//! 成功记 result、失败记 error，供事后回查。

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
    /// Unix 秒
    pub created_at: u64,
}
