# 批量导入商品 + AI 自动打标签 设计方案

> 状态：**待实现**。本文档落定后按此开发。
> 前置：`docs/design-ai-module.md` 的 AI 基础设施（AiGateway 两档端口、provider 配置管理、工具审计）已实现，本方案落地其中的用例 5.2（商品分类），采用档位 2（`run_agent` ReAct 工具循环）。

## 1. 背景与目标

- 商品目前只能逐个手动添加、逐个手动勾选标签，量大时不可用。
- 目标一：**批量导入**——粘贴一批商品名（一行一个），去重校验后批量创建，可给整批挂同一组标签。
- 目标二：**AI 自动打标签**——勾选商品，AI 从已有（启用中）标签里选择并直接写入，无需手动；支持上千商品的规模（异步任务 + 进度 + 取消）。

非目标：不做预览确认交互（AI 结果直接写入，审计表兜底）；不做智能建标签（用例 5.1，后续单独排期）。

## 2. 批量导入

### 接口

```
POST /api/products/batch
{ "names": ["佳能5D4", "索尼A7M3", ...], "tag_ids": [5] }   // tag_ids 可选
← { "created": [ProductResponse...],
    "skipped": [{"name": "...", "reason": "..."}] }
```

### 规则

- `names` 单次上限 **1000 条**，超出整批 `InvalidInput`。
- `tag_ids`（可选）整批统一应用：**提前一次性校验**，有任何一个不存在 → 整批报错，一个都不建。
- 逐条处理，单条失败不影响整批：
  - 空行：trim 后为空，静默跳过（不进 skipped）。
  - 超长/非法：`ProductName::new` 校验失败 → skipped 记原因。
  - 本批次内重复 → skipped 记「本批次内重复」。
  - 与库中已有商品重名 → skipped 记「商品名已存在」（查 `find_by_name`，不报错中断）。
- 创建的商品：`remark=None`，`tag_ids` 为整批指定值（未指定则空）。

### 实现位置

`ProductService::batch_create(names, tag_ids) -> BatchCreateResult { created, skipped }`，复用现有 `ProductRepository`，无新表。

## 3. AI 自动打标签

### 3.1 为什么用 ReAct 工具循环（档位 2）而不是 complete（档位 1）

- 每次工具执行自动落 `ai_tool_calls` 审计表（infra 已实现），AI 改了哪个商品、参数、结果全程可回查；`complete` 裸调用不记审计。
- 校验在工具 `execute` 入口做：模型瞎编 tag_id 会被拦下并作为工具结果反馈，模型可在循环内自我纠正重试——比「解析自由文本 JSON」可靠。

**关键约束：工具按批量设计，禁止每商品一次工具调用**。N 个商品 = N 轮循环会把对话历史反复重发，token 成本爆炸且易撞 `max_rounds`。目标：任意批量大小，单批循环控制在 2~3 轮。

### 3.2 工具定义（应用层 `AiTool` 实现，端口/infra 零改动）

| 工具 | 参数 | 行为 |
|---|---|---|
| `list_tags` | 无 | 实时查库返回全部 `enabled=true` 标签（id/名称/备注）。每次调用现查，不快照 |
| `apply_product_tags` | `{assignments: [{product_id, tag_ids, reason}]}` | 逐条校验：product_id 须在本次任务白名单内；tag_id 写库时现查须存在（防执行期间删标签的竞态）。合法条目整体替换该商品标签写库；非法条目剔除记 warning。返回 `{applied: n, warnings: [...]}` 反馈给模型 |

典型循环：第 1 轮模型调 `list_tags` → 第 2 轮一次性调 `apply_product_tags` 提交整批（允许分多次提交）→ 第 3 轮输出总结。`max_rounds = 8`。

模型光总结不调写工具 → 视为失败（同步接口报错「AI 未执行打标签」；异步任务该批记失败）。

### 3.3 同步路径（≤ 50 个商品）

```
POST /api/ai/classify-products
{ "product_ids": [1,2,3] }
← { "suggestions": [{product_id, tag_ids, reason}], "warnings": [...] }
```

- 上限 50，超出 `InvalidInput` 提示走异步任务接口。
- 前置校验：商品 id 存在（不存在的记 warning）；有启用标签（无 → `InvalidState`）；AI 已配置（gateway 解析时报「AI 功能未配置」）。
- 商品清单（id/名称/备注）直接写进 user prompt（固定输入，不用工具查）。
- 一次 `run_agent` 完成，同步返回工具实际写入结果。

### 3.4 异步任务路径（> 50 个商品）

输出 token 上限决定单批不能太大（50 条输出约 2k tokens，安全；1000 条约 3 万 tokens 必然截断）；总耗时分钟级，必须异步（同爬虫「异步任务不是同步请求」原则）。

```
POST /api/ai/classify-tasks            { "product_ids": [...] }  → { task }
GET  /api/ai/classify-tasks/{id}       → { task（含进度） }
POST /api/ai/classify-tasks/{id}/cancel → { task }
```

- **AiClassifyTask 实体**（`domain/ai_classify_task.rs`，内存仓储，重启即失，同 CrawlTask 模式）：
  - 状态机：`pending → running → done/failed/cancelled`，流转只走实体方法（只有 pending 能 start，只有 running 能 finish/fail/cancel）。
  - 进度字段：`total / processed / succeeded / failed`、`warnings`、当前批次、`error`、`created_at / finished_at`。
- **后台执行**：tokio task 按 50/批切片**串行**跑（不并发，防限流，同设计文档 5.4）：
  - 每批独立一次 `run_agent`（每批 `list_tags` 现查，天然感知标签增删）；
  - 批内失败（超时/解析异常/未执行写入）记 `failed` + warning，**继续下一批**，不整任务作废；
  - 批次间检查取消标记。
- **取消（秒级生效）**：cancel 接口只对 running 有效；后台用 `tokio::select!` 监听取消信号，**立刻掐断进行中的模型 HTTP 请求**，当前批作废，任务标记 `cancelled`。
- **已写入不回滚**：取消/失败前已打标签的商品保持现状，审计表可查改了什么。
- 前端同一按钮按勾选数量自动选同步/异步路径，异步显示进度条 + 取消按钮，1~2s 轮询。

### 3.5 竞态分析（任务执行中标签/商品被改）

| 场景 | 行为 |
|---|---|
| 执行中删标签 | 下一批 `list_tags` 不再有它；进行中的批次若提交了已删 tag_id，`apply_product_tags` 写库时校验剔除 + warning 反馈模型自我纠正；已关联的商品由 `product_tags` 的 `ON DELETE CASCADE` 自动清理，不悬空 |
| 执行中删完全部标签 | 该批 `list_tags` 返回空 → 任务直接 `failed`（「没有可用标签」），不空跑烧钱 |
| 执行中加标签 | 后续批次可用，无需任何处理 |
| 执行中删商品 | `apply_product_tags` 写库时找不到商品 → 该条记 warning 跳过，不中断 |

### 3.6 prompt 与输出契约

- system：角色（商品分类助手）+ 硬性规则（只能从 `list_tags` 返回的 id 中选、禁止发明标签、必须通过 `apply_product_tags` 提交、不相关的商品允许空 tag_ids）。
- user：本批商品清单 JSON `[{id, name, remark}]`。
- 工具参数即结构化输出契约（JSON Schema 由 `AiTool::parameters_schema` 声明），不解析模型自由文本。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| AI 未配置 | 接口返回「AI 功能未配置，请先在 AI 接口管理中配置」（gateway 现有行为） |
| 无启用标签 | `InvalidState`「暂无可用标签，请先创建并启用标签」 |
| 同步路径超 50 个 | `InvalidInput` 提示改用任务接口 |
| 模型超时/网络失败 | 走 provider 配置的重试；同步路径原样报错；异步路径该批记 failed 继续 |
| 模型未调写工具 | 同步路径报错；异步路径该批记 failed（warning 记录）继续 |
| 部分条目非法（未知 tag_id 等） | 剔除记 warning，合法条目正常写入 |

## 5. 前端（待爬取商品管理卡片）

- **「批量导入」按钮** → 弹窗：textarea（一行一个）+ 标签勾选区（复用现有 checkbox 渲染，可不选）→ 提交后显示「创建 N 条，跳过 M 条」+ 跳过明细，刷新表格。
- **「AI 自动打标签」按钮** → 取已勾选商品：
  - ≤50：同步调用，按钮置灰转圈，完成后 toast 汇总 + 刷新表格；
  - \>50：创建任务，卡片内显示进度条（processed/total、失败数）+「取消」按钮，1~2s 轮询直至 done/failed/cancelled。
- AI 未配置时展示后端错误并提示去「AI 接口管理」配置。

## 6. 测试策略

- `apply_product_tags` 工具 `execute` 单测：正常写入、未知 tag_id 剔除、白名单外 product_id 拒绝、部分非法不影响合法条目（内存/测试仓储，不走模型）。
- AiClassifyTask 状态机单测（非法流转报错），同 CrawlTask 测试思路。
- service 编排测试：脚本化 mock `AiGateway`（固定工具调用序列）验证同步路径与「模型不调写工具 → 报错」分支（`MockAiGateway` 固定串不够用，测试内自建桩）。

## 7. 改动文件清单

| 文件 | 改动 |
|---|---|
| `src/application/product_service.rs` | `batch_create` + `BatchCreateResult` |
| `src/domain/ai_classify_task.rs` | 新实体 + 状态机 |
| `src/domain/repository.rs` | `AiClassifyTaskRepository` trait |
| `src/domain/mod.rs` | 注册新模块 |
| `src/infrastructure/persistence/memory.rs` | 内存任务仓储实现 |
| `src/application/ai/classify_service.rs`（新） | 同步/异步/取消编排 + 两个 AiTool 实现 |
| `src/application/ai/mod.rs`、`src/application/mod.rs` | 模块注册 |
| `src/interfaces/dto.rs` | 批量导入/分类/任务 DTO |
| `src/interfaces/product_handler.rs` | `POST /api/products/batch` |
| `src/interfaces/ai_handler.rs` | classify 同步 + 任务三个接口 |
| `src/interfaces/mod.rs` | AppState + 路由注册 |
| `src/main.rs` | ClassifyService 装配（`ai_gateway` 需 clone 共享） |
| `static/index.html` / `static/app.js` | 导入弹窗、AI 按钮、进度条 + 取消 |
| `docs/design-ai-module.md` | 5.2 标记已实现（ReAct 直接写入模式） |
| `AGENTS.md` | 架构树与 AI 决策段同步 |
