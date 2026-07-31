# AI 模块设计方案

> 状态：**基础设施已实现**（API 配置管理 + AiGateway 端口 + rig 实现 + 工具审计）；具体 AI 用例（智能建标签、商品分类等）未实现，确认后排期。

## 1. 背景与目标

后续多个功能会用到 AI 能力，例如：

- 导入一批商品名/文本，AI 自动建议一套标签体系（建哪些标签、每个标签什么含义）
- 用已有标签给一批商品自动分类（建议每个商品挂哪些 tag_ids）
- 将来可能还有：爬取结果摘要、价格异常分析、回收价建议等

目标：现在就定好 AI 能力的**接入架构和扩展约定**，让以后每加一个 AI 功能都是「加一个 service + 一个 prompt 模板」，不动底层。

非目标：本次不实现任何具体 AI 用例、不接真实模型、不做向量检索/embedding。

## 2. 核心设计原则

1. **AI 网关 = 端口 + 防腐层**：和 `XianYuGateway` 完全同一模式。trait 定义在 `application/ports.rs`，HTTP 调用、鉴权、重试等易变细节全在 `infrastructure/ai_gateway.rs`。内层不知道用的是哪家模型。
2. **AI 通过工具直接产生结果，完全自动**：AI 可调用的工具包括写操作（建标签、改商品、入队等），执行不做人工确认。**兜底机制是审计**：所有工具调用落 `ai_tool_calls` 表（工具名/参数/结果或错误/耗时），前端可回查，出问题可追溯。对 AI 输出不放心时，用例仍可自行选择「预览 → 确认」的交互（如 5.1/5.2），但那是用例层的 UX 选择，不是架构强制。
3. **mock 优先**：`MockAiGateway` 返回固定输出，开发和单测不依赖真实 API、不花钱。
4. **结构化输出 + 严格解析**：prompt 要求模型只返回 JSON，应用层解析并逐字段校验（标签名长度、tag_id 是否存在等），不合格就报错或重试，不信任模型任何自由文本。

## 3. 架构位置（DDD-Lite 不变）

```
application/
├── ports.rs              # 新增 trait AiGateway（与 XianYuGateway 并列）
├── ai/
│   ├── tag_suggest_service.rs   # 用例：建议标签体系（示例，后续实现）
│   └── classify_service.rs      # 用例：商品分类建议（示例，后续实现）
infrastructure/
├── ai_gateway.rs         # AiGateway 实现：Mock / OpenAiCompatible
└── config.rs             # 新增 AI_* 环境变量
interfaces/
└── ai_handler.rs         # /api/ai/* 路由（按用例加）
```

**关键取舍：AiGateway 提供两档能力**——

```rust
#[async_trait]
pub trait AiGateway: Send + Sync {
    /// 档位 1：单次对话补全（够用例 5.1/5.2 这种「一次出建议」的场景）
    async fn complete(&self, system: &str, user: &str) -> Result<String, DomainError>;

    /// 档位 2：带工具的 agent 循环（ReAct）。tools 由应用层定义，
    /// 实现方负责「模型请求工具 → 执行 → 结果回填 → 再调用」的循环，直到模型给出最终答案。
    async fn run_agent(
        &self,
        system: &str,
        user: &str,
        tools: &[Arc<dyn AiTool>],
        max_rounds: u32,
    ) -> Result<String, DomainError>;
}

/// 应用层定义的工具端口：名称/参数 schema/执行逻辑都在这里，
/// infrastructure 只负责把它翻译成模型厂商的 function/tool 规格。
#[async_trait]
pub trait AiTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value; // JSON Schema
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value, DomainError>;
}
```

为什么不按用例定义高层方法（如 `suggest_tags()`）：高层契约每加一个用例就要改端口、改 mock、改真实实现，违反「扩展不改底层」。端口稳定不变；**prompt 模板、响应解析、工具集合都放在各用例的 service 里**——这些本质是业务规则的一部分，属于应用层。`max_rounds` 封顶防模型陷入工具调用死循环。

## 3.1 库选型（联网调研结论）

| 候选 | 说明 | 结论 |
|---|---|---|
| **[rig-core](https://github.com/0xPlaygrounds/rig)** | Rust 生态事实标准的 LLM 框架：统一 API 覆盖 20+ 提供商（OpenAI、DeepSeek、Moonshot/Kimi、Anthropic、Gemini、Ollama、OpenRouter…）、类型安全 Tool trait + JSON schema 自动生成、自带 agent 循环（ReAct）、结构化输出 Extractor、流式；维护活跃（2026 仍在高频发版），[官方站](https://rig.rs/) | **推荐** |
| [langchain-rust](https://github.com/Abraxas-365/langchain-rust) | LangChain 的 Rust 移植，支持 OpenAI function calling | 生态和活跃度明显弱于 rig，不选 |
| [llm-chain](https://github.com/sobelio/llm-chain) | 链式 prompt 编排，偏老 | 近年少维护，不选 |
| [async-openai](https://github.com/64bit/async-openai) / 手写 reqwest | 裸 SDK，agent 循环自己写 | 循环本身不复杂（[参考](https://vadim.blog/two-paradigms-multi-agent-ai-rust-vs-claude-teams)），但 rig 已解决且带多提供商，没必要重复造 |

**决定：infrastructure 的真实实现基于 `rig-core`**。理由：工具调用/ReAct 是它的核心能力而非拼装货；内置 DeepSeek/Moonshot 等国内提供商也支持任意 OpenAI 兼容端点；结构化输出（Extractor）正好满足「严格 JSON 解析」；将来要 embedding/向量检索有配套 crate（`rig-sqlite` 等），不用换框架。

**边界纪律**：rig 的类型只许出现在 `infrastructure/ai_gateway.rs`。`AiGateway`/`AiTool` 端口是我们自己的抽象，infrastructure 里做一层适配（把 `Arc<dyn AiTool>` 桥接成 rig 的 tool 定义）。这样将来 rig 停更或要换实现，内层零改动——和第 2 节的防腐层原则一致。

## 4. 配置

**AI 接口配置入库管理（`ai_providers` 表）**：前端「AI 接口管理」卡片维护多供应商配置（名称/base_url/api_key/model/超时/重试），一条为默认；支持设为默认、测试连通性。密钥响应掩码显示（`sk-****1234`），明文存本地 SQLite（个人本地工具，注意 db 文件别外发）。

环境变量降级为**兜底**（无 DB 默认配置时生效）：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `AI_BASE_URL` | `https://api.openai.com/v1` | OpenAI 兼容端点 |
| `AI_API_KEY` | - | 兜底密钥 |
| `AI_MODEL` | `gpt-4o-mini` | 兜底模型名 |

优先级：**DB 默认配置 > 环境变量**；两者都无 → 接口返回「AI 功能未配置」。真实实现基于 `rig-core`（见 3.1），走 OpenAI 兼容协议：国内主流模型（DeepSeek、通义千问、Kimi、智谱）全都提供该协议的端点，一套代码通吃，换供应商只改配置。

## 5. 用例模式（以两个例子说明，确认后实现）

### 5.1 智能建标签（导入 → 建议 → 确认创建）

```
POST /api/ai/suggest-tags   { "texts": ["佳能5D4机身", "索尼A7M3", ...] }
← { "suggestions": [
      { "name": "相机", "remark": "机身类", "matched": ["佳能5D4机身", ...] },
      { "name": "镜头", ... } ] }
```

- service 把输入文本列表 + 已有标签名（避免重复建议）拼进 prompt，要求返回 JSON 数组
- 解析校验：标签名查重（对已有标签、建议内部重复都给提示）
- 前端展示建议清单，用户勾选/改名后批量调已有的 `POST /api/tags` 创建

### 5.2 商品分类（已有标签 → 建议 → 确认应用）

```
POST /api/ai/classify-products   { "product_ids": [1,2,3] }
← { "suggestions": [ { "product_id": 1, "tag_ids": [5,6], "reason": "..." } ] }
```

- service 装入商品名+备注、全部启用标签，要求模型为每个商品从**已有标签 id 集合**中选择（禁止发明新标签）
- 解析校验：tag_id 必须存在，不存在的丢弃并记录
- 前端逐条展示建议（可改），确认后调已有的 `PUT /api/products/{id}` 应用

### 5.3 带工具的 agent 场景（举例，说明档位 2 的用法）

「帮我把超过 7 天没爬的商品找出来加入队列」这类自然语言指令，用 `run_agent`：

- 应用层提供工具集：`list_products`（查商品+统计）、`list_tags`、`enqueue_products`（直接入队）
- 模型自主决定：调 `list_products` 拿数据 → 筛选 → 调 `enqueue_products` 入队 → 给出总结
- 工具直接产生结果（含写库），每次调用落 `ai_tool_calls` 审计表，前端「AI 工具调用记录」可回查
- `max_rounds` 建议默认 8，防止模型在工具循环里空转烧钱

### 5.4 共同规则

- 批量上限：单次建议最多 100 条数据，超出要求分批（防 token 爆炸和超时）
- 串行调用，不并发打 API（和爬虫串行同一思路，防限流）
- 工具调用一律落审计日志；`complete` 的裸调用不记（没有工具执行）

## 6. 错误处理

| 场景 | 行为 |
|---|---|
| 未配置 API key | 接口返回「AI 功能未配置，请设置 AI_API_KEY」 |
| 网络失败/超时 | 按 `AI_MAX_RETRIES` 重试，最终返回错误信息 |
| 模型返回非 JSON / 缺字段 | 重试；仍失败返回「AI 返回格式异常，请重试」 |
| JSON 合法但内容非法（未知 tag_id 等） | 丢弃非法条目，正常返回其余 + 警告列表 |

## 7. 测试策略

- `MockAiGateway` 实现两档端口：`complete` 按 system prompt 特征返回固定 JSON；`run_agent` 用脚本化的固定工具调用序列模拟 ReAct 循环。端到端验证不花钱
- prompt 解析器单独抽函数（`parse_suggestions(json) -> Result<...>`），纯函数单测：正常、缺字段、多余字段、非法 id、模型套话包裹 JSON 等 case
- 工具（`AiTool` 实现）的 `execute` 用内存/测试库仓储单测，不走模型
- service 单测用 mock 端口，不走 HTTP

## 8. 扩展约定（以后加 AI 功能的固定动作）

1. `application/ai/` 下新建 service，持有 `Arc<dyn AiGateway>` + 需要的仓储
2. prompt 模板写成 service 内的常量字符串，输出契约（JSON schema 说明）一并写清
3. 响应解析函数独立、可单测
4. `interfaces/` 加 handler + `/api/ai/xxx` 路由
5. 前端交互由用例自定：可以「预览 → 确认」，也可以让 AI 直接执行（执行结果 + 审计记录可查）
6. **不改** `AiGateway` trait、不改 infrastructure 实现

## 9. 待你确认的开放问题

1. **模型供应商**：配置走前端「AI 接口管理」，你打算用哪家（DeepSeek / 千问 / Kimi / OpenAI）？只影响你自己填的配置，代码无关。
2. ~~AI 调用记录是否落库审计~~ → 已确认：**落 `ai_tool_calls` 表，已实现**。
3. **分类建议的写入方式**：确认后「整体替换商品标签」还是「在现有标签上追加」？默认整体替换（与 PUT /products 现有语义一致）。
4. 两个示例用例之外，近期还想先实现哪个 AI 功能？
