//! AI 网关实现：基于 rig-core 的 OpenAI 兼容端点。
//!
//! 本文件是 rig 类型在项目中的唯一落脚点：
//! - 用 OpenAI 兼容协议接入任意供应商（DeepSeek/千问/Kimi 等）；
//! - 单次补全直接调 Chat Completions API；
//! - 带工具的 agent 循环在本文件中手写（rig-core 0.41 把高级 agent 运行时拆到了 `rig-agent` crate，
//!   这里只用它提供的补全 + ToolDefinition + Message 抽象）。

use std::sync::Arc;

use async_trait::async_trait;
use rig_core::client::CompletionClient;
use rig_core::completion::{AssistantContent, CompletionModel, ToolDefinition};
use rig_core::message::{Message, ToolResult, ToolResultContent, UserContent};
use rig_core::OneOrMany;
use rig_core::providers::openai;

use crate::application::ports::{AiEnvFallback, AiGateway, AiTool};
use crate::domain::ai_provider::{AiProvider, BaseUrl, ModelName, ProviderName};
use crate::domain::ai_tool_call::NewAiToolCall;
use crate::domain::error::DomainError;
use crate::domain::repository::{AiProviderRepository, AiToolCallRepository};

/// 真实实现：每次调用时读取默认配置（DB → 环境变量兜底），即时生效。
#[allow(dead_code)]
pub struct RigAiGateway {
    providers: Arc<dyn AiProviderRepository>,
    calls: Arc<dyn AiToolCallRepository>,
    env: AiEnvFallback,
}

#[allow(dead_code)]
impl RigAiGateway {
    pub fn new(
        providers: Arc<dyn AiProviderRepository>,
        calls: Arc<dyn AiToolCallRepository>,
        env: AiEnvFallback,
    ) -> Self {
        Self {
            providers,
            calls,
            env,
        }
    }

    /// 解析当前生效配置：数据库默认 > 环境变量兜底
    #[allow(dead_code)]
    async fn resolve_provider(&self) -> Result<AiProvider, DomainError> {
        if let Some(p) = self.providers.find_default().await? {
            tracing::debug!(
                "AI 使用数据库默认 provider: name={}, base_url={}, model={}",
                p.name,
                p.base_url,
                p.model
            );
            return Ok(p);
        }
        if let Some(key) = &self.env.api_key {
            tracing::debug!(
                "AI 使用环境变量兜底: base_url={}, model={}",
                self.env.base_url,
                self.env.model
            );
            let now = crate::domain::crawl_task::now_unix();
            // 环境变量属部署配置，非法值归为基础设施错误
            let env_err = |e: DomainError| {
                DomainError::Infrastructure(format!("AI 环境变量配置非法: {e}"))
            };
            return Ok(AiProvider {
                id: 0,
                name: ProviderName::new("env-fallback").map_err(env_err)?,
                base_url: BaseUrl::new(self.env.base_url.clone()).map_err(env_err)?,
                api_key: Some(key.clone()),
                model: ModelName::new(self.env.model.clone()).map_err(env_err)?,
                timeout_secs: 60,
                max_retries: 2,
                is_default: false,
                created_at: now,
                updated_at: now,
            });
        }
        tracing::warn!("AI 功能未配置：无数据库默认 provider，也未设置 AI_API_KEY 环境变量");
        Err(DomainError::InvalidState("AI 功能未配置".into()))
    }

    /// 用指定配置构建 OpenAI 兼容 Chat Completions client
    fn build_client(provider: &AiProvider) -> Result<openai::CompletionsClient, DomainError> {
        let key = provider
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| DomainError::InvalidState("AI 配置未设置 API Key".into()))?;
        let client = openai::Client::builder()
            .api_key(key)
            .base_url(provider.base_url.as_str())
            .build()
            .map_err(|e| DomainError::Infrastructure(format!("rig client: {e}")))?
            .completions_api();
        Ok(client)
    }

    /// 把应用层工具翻译成 rig 的 ToolDefinition
    #[allow(dead_code)]
    fn tool_definitions(tools: &[Arc<dyn AiTool>]) -> Vec<ToolDefinition> {
        tools
            .iter()
            .map(|t| ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }
}

/// 执行单个工具并落审计日志；错误也会返回，让调用方决定是否回填给模型
#[allow(dead_code)]
async fn execute_tool(
    calls: Arc<dyn AiToolCallRepository>,
    tool: &Arc<dyn AiTool>,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, DomainError> {
    tracing::debug!("执行工具 {}: args={}", tool.name(), arguments);
    let start = std::time::Instant::now();
    let result = tool.execute(arguments.clone()).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    tracing::debug!("工具 {} 执行耗时 {} ms", tool.name(), duration_ms);

    let (result_json, error_text) = match &result {
        Ok(v) => (Some(v.to_string()), None),
        Err(e) => (None, Some(e.to_string())),
    };

    if let Err(e) = calls
        .create(&NewAiToolCall {
            tool_name: tool.name().to_string(),
            arguments: arguments.to_string(),
            result: result_json,
            error: error_text,
            duration_ms,
        })
        .await
    {
        tracing::warn!("AI 工具调用审计落库失败: {e}");
    }

    result
}

#[async_trait]
impl AiGateway for RigAiGateway {
    async fn is_available(&self) -> bool {
        self.resolve_provider().await.is_ok()
    }

    async fn complete(&self, system: &str, user: &str) -> Result<String, DomainError> {
        let provider = self.resolve_provider().await?;
        self.complete_with(&provider, system, user).await
    }

    async fn complete_with(
        &self,
        provider: &AiProvider,
        system: &str,
        user: &str,
    ) -> Result<String, DomainError> {
        tracing::debug!("AI complete 请求: model={}, user_len={}", provider.model, user.len());
        let client = Self::build_client(provider)?;
        let model = client.completion_model(provider.model.as_str());
        let response = model
            .completion_request(Message::user(user))
            .preamble(system.to_string())
            .send()
            .await
            .map_err(|e| DomainError::Infrastructure(format!("AI 请求失败: {e}")))?;

        let texts: Vec<String> = response
            .choice
            .iter()
            .filter_map(|c| match c {
                AssistantContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        let reply = texts.join("");
        tracing::debug!("AI complete 响应长度: {}", reply.len());
        tracing::trace!("AI complete 响应内容: {}", reply);
        Ok(reply)
    }

    async fn run_agent(
        &self,
        system: &str,
        user: &str,
        tools: &[Arc<dyn AiTool>],
        max_rounds: u32,
    ) -> Result<String, DomainError> {
        let provider = self.resolve_provider().await?;
        let client = Self::build_client(&provider)?;
        let model = client.completion_model(provider.model.as_str());
        let tool_defs = Self::tool_definitions(tools);
        tracing::debug!(
            "AI agent 启动: model={}, tools={:?}, max_rounds={}",
            provider.model,
            tools.iter().map(|t| t.name()).collect::<Vec<_>>(),
            max_rounds
        );

        // 对话历史：先放入初始 system/user；后续每轮追加 assistant(tool_call) + user(tool_result)
        let mut history: Vec<Message> = vec![];

        for round in 0..max_rounds {
            tracing::debug!("AI agent 第 {} 轮开始", round + 1);
            let prompt = if round == 0 {
                Message::user(user)
            } else {
                // 非首轮推动模型继续，并提醒它必须用完写工具再总结
                Message::user("继续。如果你还没有通过工具提交全部结果，请先调用相应工具提交，全部提交完再输出简短总结。")
            };

            let response = model
                .completion_request(prompt)
                .preamble(system.to_string())
                .messages(history.clone())
                .tools(tool_defs.clone())
                .send()
                .await
                .map_err(|e| DomainError::Infrastructure(format!("AI agent 请求失败: {e}")))?;

            // 首轮 user 消息（商品清单等关键输入）必须进入历史，
            // 否则第二轮起模型只能看到工具往返，看不到原始请求
            if round == 0 {
                history.push(Message::user(user));
            }

            let mut texts = Vec::new();
            let mut tool_calls = Vec::new();
            for item in response.choice.iter() {
                match item {
                    AssistantContent::Text(t) => {
                        tracing::trace!("AI agent 第 {} 轮文本输出: {}", round + 1, t.text);
                        texts.push(t.text.clone());
                    }
                    AssistantContent::ToolCall(tc) => {
                        tracing::debug!(
                            "AI agent 第 {} 轮调用工具: {} args={}",
                            round + 1,
                            tc.function.name,
                            tc.function.arguments
                        );
                        tool_calls.push(tc.clone());
                    }
                    _ => {}
                }
            }

            // 先把 assistant 这一整轮（含文本+工具调用）写进历史
            let mut assistant_content: Vec<AssistantContent> = Vec::new();
            if !texts.is_empty() {
                assistant_content.push(AssistantContent::text(texts.join("")));
            }
            for tc in &tool_calls {
                assistant_content.push(AssistantContent::ToolCall(tc.clone()));
            }
            if !assistant_content.is_empty() {
                history.push(Message::Assistant {
                    id: None,
                    content: OneOrMany::many(assistant_content)
                        .expect("assistant content 不应为空"),
                });
            }

            if tool_calls.is_empty() {
                // 没有工具调用，直接返回文本
                let reply = texts.join("");
                tracing::debug!("AI agent 第 {} 轮无工具调用，直接返回（长度 {}）", round + 1, reply.len());
                return Ok(reply);
            }

            // 执行工具调用并把结果回填
            for tc in tool_calls {
                let tool = tools
                    .iter()
                    .find(|t| t.name() == tc.function.name)
                    .ok_or_else(|| {
                        DomainError::InvalidState(format!("未知工具: {}", tc.function.name))
                    })?;

                let tool_result = match execute_tool(self.calls.clone(), tool, tc.function.arguments).await {
                    Ok(v) => {
                        tracing::trace!("工具 {} 执行结果: {}", tc.function.name, v);
                        ToolResultContent::json(v)
                    }
                    Err(e) => {
                        tracing::debug!("工具 {} 执行失败: {e}", tc.function.name);
                        ToolResultContent::text(format!(
                            "工具执行失败: {e}"
                        ))
                    }
                };

                history.push(Message::User {
                    content: OneOrMany::one(UserContent::ToolResult(ToolResult {
                        id: tc.id,
                        call_id: tc.call_id,
                        content: OneOrMany::one(tool_result),
                    })),
                });
            }
        }

        tracing::warn!("AI agent 达到最大轮数 {} 仍未给出最终答案", max_rounds);
        Err(DomainError::InvalidState(
            "AI agent 工具调用轮数达到上限仍未给出最终答案".into(),
        ))
    }
}

/// Mock 实现：不联网，固定返回字符串，供后续单测与开发使用。
#[allow(dead_code)]
pub struct MockAiGateway;

#[async_trait]
impl AiGateway for MockAiGateway {
    async fn is_available(&self) -> bool {
        true
    }

    async fn complete(&self, _system: &str, _user: &str) -> Result<String, DomainError> {
        Ok("mock-complete".into())
    }

    async fn complete_with(
        &self,
        _provider: &AiProvider,
        _system: &str,
        _user: &str,
    ) -> Result<String, DomainError> {
        Ok("mock-complete-with".into())
    }

    async fn run_agent(
        &self,
        _system: &str,
        _user: &str,
        _tools: &[Arc<dyn AiTool>],
        _max_rounds: u32,
    ) -> Result<String, DomainError> {
        Ok("mock-agent".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::sqlite::{
        self, SqliteAiToolCallRepository,
    };

    struct EchoTool;

    #[async_trait]
    impl AiTool for EchoTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "echo args" }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value, DomainError> {
            Ok(args)
        }
    }

    struct FailTool;

    #[async_trait]
    impl AiTool for FailTool {
        fn name(&self) -> &str { "fail" }
        fn description(&self) -> &str { "always fail" }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, _args: serde_json::Value) -> Result<serde_json::Value, DomainError> {
            Err(DomainError::InvalidInput("boom".into()))
        }
    }

    #[tokio::test]
    async fn tool_call_audit_logs_success_and_failure() {
        let pool = sqlite::connect(":memory:").await.unwrap();
        let calls: Arc<dyn AiToolCallRepository> = Arc::new(SqliteAiToolCallRepository::new(pool));

        let echo: Arc<dyn AiTool> = Arc::new(EchoTool);
        let res = execute_tool(calls.clone(), &echo, serde_json::json!({"x": 1})).await;
        assert!(res.is_ok());

        let fail: Arc<dyn AiTool> = Arc::new(FailTool);
        let res = execute_tool(calls.clone(), &fail, serde_json::json!({})).await;
        assert!(res.is_err());

        let logs = calls.list_paginated(0, 10, None, None).await.unwrap().items;
        assert_eq!(logs.len(), 2);

        let success_log = logs.iter().find(|l| l.tool_name == "echo").unwrap();
        assert!(success_log.result.is_some());
        assert!(success_log.error.is_none());

        let fail_log = logs.iter().find(|l| l.tool_name == "fail").unwrap();
        assert!(fail_log.result.is_none());
        assert!(fail_log.error.as_ref().unwrap().contains("boom"));
    }
}
