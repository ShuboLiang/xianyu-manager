# xianyu-manager

闲鱼爬虫与商品管理系统：Rust HTTP Web 服务（axum）+ 内置静态前端。

## 构建与运行

```bash
# 前端：React + TS 源码在 web/，构建产物输出到 static/
cd web && npm install && npm run build

# 后端（同时托管 static/ 下的前端）
cargo build
cargo run        # 默认 http://127.0.0.1:3000
```

前端日常开发：

```bash
cd web && npm run dev   # http://127.0.0.1:5173，/api 自动代理到 127.0.0.1:3000（需后端已启动）
```

环境变量配置（见 `src/infrastructure/config.rs`）：

| 变量            | 默认值                      | 说明                                                  |
| --------------- | --------------------------- | ----------------------------------------------------- |
| `HOST`          | `127.0.0.1`                 | 监听地址                                              |
| `PORT`          | `3000`                      | 监听端口                                              |
| `STATIC_DIR`    | `static`                    | 前端静态文件目录                                      |
| `GATEWAY`       | `webbridge`                 | `webbridge`=真实抓取（WebBridge 浏览器 + AI 筛选，默认）；`mock`=假数据网关；`http`=真实接口（未实现） |
| `WEBBRIDGE_URL` | `http://127.0.0.1:10086`    | `GATEWAY=webbridge` 时的 Kimi WebBridge daemon 地址   |
| `WEBBRIDGE_BIN_PATH` | 自动推导：Windows `%USERPROFILE%\.kimi-webbridge\bin\kimi-webbridge.exe`，其他 `~/.kimi-webbridge/bin/kimi-webbridge` | `GATEWAY=webbridge` 时若 daemon 未运行，自动执行该路径的 `... start` 拉起 |
| `RECYCLE_FACTOR`| `0.9`                       | 回收价系数：回收价 = 中位数 × 系数（0,1]              |
| `XIANYU_COOKIE` | -                           | `GATEWAY=http` 时的闲鱼登录态 Cookie                  |
| `DATABASE_PATH` | `data/xianyu.db`            | SQLite 数据库文件路径（目录自动创建）                 |
| `AI_API_KEY`    | -                           | AI 环境变量兜底密钥（未配置 DB 默认 provider 时生效） |
| `AI_BASE_URL`   | `https://api.openai.com/v1` | AI 环境变量兜底端点（OpenAI 兼容）                    |
| `AI_MODEL`      | `gpt-4o-mini`               | AI 环境变量兜底模型名                                 |

## 架构（DDD-Lite，四层）

依赖规则：**外层可以依赖内层，内层绝不知道外层的存在**。`domain/` 不 import 任何外部框架（axum/reqwest/tokio 一律不出现）。跨层注入统一用 `Arc<dyn Trait>`。

```
src/
├── interfaces/          # 接口层：axum 路由、handler、dto（serde 注解只属于这里）
│   ├── mod.rs           #   build_router() + AppState（应用服务句柄）
│   ├── dto.rs           #   HTTP 请求/响应结构，与 domain 模型解耦
│   ├── crawl_handler.rs #   POST /api/crawl、GET /api/crawl/{id}
│   ├── item_handler.rs  #   GET /api/items（分页；normalize_page 钳制页码，供各 handler 复用）
│   ├── tag_handler.rs   #   GET/POST /api/tags、GET/PUT/DELETE /api/tags/{id}
│   ├── product_handler.rs#  GET/POST /api/products（列表分页 + 服务端排序）、GET/PUT/DELETE /api/products/{id}、GET /api/products/{id}/latest-items（最后一轮抓取明细）
│   ├── queue_handler.rs #   /api/queues 系列：预览、入队、暂停/恢复/取消、全部暂停/恢复、追加条目
│   ├── ai_handler.rs    #   /api/ai 系列：provider 增删改查、设为默认、连通性测试、工具调用审计（分页）、GET/PUT /api/ai/crawl-prompt（自定义抓取提示词）
│   └── stats_handler.rs #   GET /api/stats（KPI 概览统计）
├── application/         # 应用层：用例编排，不含业务规则
│   ├── ports.rs         #   端口 trait：XianYuGateway、AiGateway/AiTool（防腐层）
│   ├── crawl_service.rs #   创建抓取任务 → 后台 tokio task 逐页抓取 → 落库
│   ├── item_service.rs  #   商品列表查询（分页）
│   ├── tag_service.rs   #   标签 CRUD（重名冲突校验、部分字段更新）
│   ├── product_service.rs#  待爬取商品 CRUD（重名校验、标签存在校验）+ 分页排序列表
│   ├── queue_service.rs #   抓取队列：选择器解析、入队去重、全局唯一 worker 串行消费
│   ├── ai_provider_service.rs # AI 供应商配置管理 + 连通性测试
│   ├── ai_settings_service.rs # AI 应用设置：用户自定义抓取提示词读写（存 app_settings KV 表）
│   ├── ai_tool_call_service.rs # AI 工具调用审计查询（分页）
│   ├── ai/            #   AI 用例：classify_service（自动打标签）/ crawl_agent_service（AI 驱动抓取 + 两个工具）
│   └── stats_service.rs #   KPI 概览统计（商品总数 / 24h 抓取 / 最后爬取时间）
├── domain/              # 领域层：零外部依赖
│   ├── item.rs          #   Item 实体，Keyword/PageRange 值对象（含校验）
│   ├── crawl_task.rs    #   CrawlTask 实体：状态流转规则只写在实体方法里
│   ├── tag.rs           #   Tag 实体（TagName 值对象），enabled=false 的标签不参与抓取
│   ├── product.rs       #   Product 实体（待爬取商品）：名称/标签/备注 + 爬取统计字段
│   ├── crawl_queue.rs   #   CrawlQueue/CrawlEntry 实体、Selector 值对象、状态机
│   ├── ai_provider.rs   #   AiProvider 实体（OpenAI 兼容端点配置）
│   ├── ai_tool_call.rs  #   AiToolCall 审计实体
│   ├── repository.rs    #   仓储端口 trait：ItemRepository / CrawlTaskRepository / TagRepository / ProductRepository / QueueRepository / AiProviderRepository / AiToolCallRepository / SettingsRepository（KV 设置）
│   └── error.rs         #   DomainError，全项目统一错误语义
└── infrastructure/      # 基础设施层：实现内层定义的 trait
    ├── config.rs        #   环境变量配置
    ├── xianyu_gateway.rs#   XianYuGateway 实现：Mock / Http（真实接口待实现）
    ├── webbridge_client.rs#  WebBridge 客户端：驱动本机真实浏览器搜闲鱼并提取候选（也实现 XianYuGateway）
    ├── ai_gateway.rs    #   AiGateway 实现：基于 rig-core 的 OpenAI 兼容端点 + 手写 ReAct 工具循环
    └── persistence/
        ├── memory.rs    #   内存仓储：抓取任务、AI 分类任务（重启即失）；ItemRepository 内存实现保留为备选
        └── sqlite.rs    #   SQLite 仓储：抓取商品数据、标签、待爬取商品、抓取队列+条目、AI 配置与审计、app_settings KV 设置（共享连接池，启动自动建表）
static/                  # 前端构建产物（由 web/ 执行 npm run build 生成，axum 托管，勿手改）
web/                     # 前端源码：React 19 + TS + Vite 7 + Tailwind + shadcn/ui
├── src/types/generated/ #   ts-rs 从 dto.rs 自动生成的类型（cargo test export_bindings，勿手改）
├── src/types/api.ts     #   类型的友好别名 re-export + 手写 ApiResponse<T> 包装
├── src/lib/api.ts       #   fetch 封装：解包 ApiResponse<T>，code!==0 抛错
└── src/sections/        #   页面区块：KpiStrip（概览条）/ QueuesCard / ProductsCard / TagsCard / ItemsCard / AiCard + Pager（通用分页条）+ SkeletonRows（共享骨架行）
```

前端约定：

- `web/vite.config.ts`：dev 端口 5173（避让后端 3000），`/api` 代理到 `127.0.0.1:3000`；`build.outDir` 指向 `../static` 且 `emptyOutDir`，构建即覆盖旧产物。
- 前端用 `react-router` 的 **HashRouter**（避免 axum 静态托管需要路由回退）+ 左侧固定导航（移动端为 Sheet 抽屉）：五个页面 = 概览（KpiStrip + QueuesCard）/ 商品管理 / 标签管理 / 抓取数据 / AI。全局状态仍在 `App.tsx` 集中管理，路由页面只是视图，切页不打断队列轮询；通知用 sonner toast。
- 队列轮询逻辑在 `App.tsx::loadQueues`：有 waiting/running 队列时每 2 秒自刷新，刚全部结束时补刷商品统计与原始数据。
- **前后端类型自动同步（ts-rs）**：`interfaces/dto.rs` 的 DTO 全部 `#[derive(TS)]` + `#[ts(export)]`，运行 `cargo test export_bindings` 会把 TS 类型写入 `web/src/types/generated/`（导出目录配置在 `.cargo/config.toml` 的 `TS_RS_EXPORT_DIR`）。前端 `web/src/types/api.ts` 只做别名 re-export（`TagResponse`→`Tag` 等）与 `ApiResponse<T>` 手写包装，**不再手工维护字段**。改 dto.rs 后必须重跑导出 + `npm run build`。
  - 新增 DTO 的 `i64`/`u64`/`Vec<i64>` 字段必须标 `#[ts(type = "number")]` / `#[ts(type = "Array<number>")]`（ts-rs 默认映射 bigint，与 JSON 实际序列化不符）。
  - `ApiResponse<T>` 泛型不经 ts-rs 导出；`QueueResponse.status` 在 Rust 是 `String`，前端 `api.ts` 里收窄为 `QueueStatus` 联合类型。

## 核心设计决策

- **抓取是异步任务，不是同步请求**：`POST /api/crawl` 只创建任务并返回句柄，后台 tokio task 执行抓取，前端轮询 `GET /api/crawl/{id}`。防止多页抓取时 HTTP 超时。
- **任务状态机**：`Pending → Running → Done/Failed`。只有 Pending 能 `start()`，只有 Running 能 `finish()`；规则在 `CrawlTask` 实体方法里，不允许在别处直接改状态。
- **闲鱼网关是端口+实现（防腐层）**：`XianYuGateway` trait 定义在 `application/ports.rs`，实现（登录态、mtop 签名、解析）在 `infrastructure/xianyu_gateway.rs`。开发用 `GATEWAY=mock`。
- **仓储端口**：`ItemRepository` / `CrawlTaskRepository` / `TagRepository` trait 在 domain。抓取商品数据（items 表，id=详情页 URL，重复抓取 INSERT OR REPLACE 覆盖）、标签、商品、队列、AI 配置/审计均已落 SQLite（`infrastructure/persistence/sqlite.rs`，连接时自动建表）；抓取任务与 AI 分类任务仍是内存实现（重启即失，可接受）。换存储时新增实现并在 `main.rs` 替换，内层零改动。
- **标签管理**：标签（`domain/tag.rs`）管理「爬虫爬哪一类商品」，目前只含名称/启用状态/备注；抓取策略（关键词、频率、页数、过滤规则等）后续挂在标签上扩展。`enabled=false` 的标签届时不参与抓取。标签名全局唯一，冲突返回 `DomainError::Conflict`。
- **待爬取商品管理**：商品（`domain/product.rs`）管理「要爬哪些商品」。基础信息：名称（唯一，冲突返回 `Conflict`）、标签（**多对多**，`tag_ids: Vec<i64>`，默认空=无标签；存 `product_tags` 关联表，删除商品或标签时外键 `ON DELETE CASCADE` 自动清理关联）、备注。统计字段（中位数/均价/爬取数量/最后爬取时间/回收价格）由爬取结果写入（`Product::record_crawl_result`），未爬取时为 null；**回收价例外地允许手动设置/清空**（更新接口 `recycle_price`：不传=不修改，null=清空，数值=设定，校验在 `Product::set_recycle_price`，下一轮爬取会覆盖手动值；前端商品表格回收价单元格行内编辑）。更新接口的 `tag_ids`：不传=不修改，空数组=清空全部标签，非空数组=整体替换。
- **删除语义**：数据库层一律 `CASCADE` 兜底（删标签/删商品只清关联，另一方不受影响）；交互层做影响提示——`GET /api/tags/{id}/products` 返回使用该标签的商品，前端删除标签前在确认框中列出受影响商品。不做「阻止删除」。
- **列表分页**：`/api/items`、`/api/products`、`/api/ai/tool-calls` 三个列表接口服务端分页，统一 `page`（从 1 起）/ `page_size`（默认 20，clamp 1..=100，钳制逻辑在 `item_handler::normalize_page`），响应为 `PageResponse<T> { items, total, page, page_size }`（`PageResponse<T>` 泛型不经 ts-rs 导出，前端 `api.ts` 手写）。商品列表支持服务端排序（`sort_by`/`sort_dir`，排序列白名单枚举 `ProductSortColumn` 在 `domain/repository.rs`，SQL 空值沉底）；前端列头排序只是改查询条件回第 1 页。tags 和 queues 不分页（标签是全局选项源，队列轮询需全量活跃视图）。KPI 概览不从列表数据推导，由 `GET /api/stats` 提供（product_count / crawled_today=滚动 24h 窗口 / last_crawled_at）。
- **抓取队列**（详细方案见 `docs/design-crawl-queue.md`）：
  - 队列 = 商品 id 快照（`crawl_entries`），入队后改标签/规则不影响本队列；删除商品时其条目由 worker 标记为 `skipped`，不阻塞队列。
  - 入队方式二选一：`selector`（`tag_all`/`tag_any`/`tag_exclude`/`stale_days`，维度间 AND）或 `product_ids`（手动勾选）；入队前可 `POST /api/queues/preview` 预览命中与跳过。
  - **全局去重**：同一商品只允许存在于一个活跃（waiting/running/paused）队列中，重复者入队响应的 `skipped` 列出；done/cancelled 队列不占位。
  - **全局唯一 worker**：`QueueService::start_worker` 在 `main.rs` 启动时拉起，串行消费，任意时刻最多一个 running 队列；running 结束/暂停后最早的 waiting 自动顶上。条目间隔 `interval_secs`（睡眠切成 1 秒小片段以便及时响应暂停）。
  - 队列状态机：`waiting → running → paused/done/cancelled`，`paused → waiting`（恢复=重新排队，无差别恢复）；暂停是**优雅暂停**——当前条目跑完才停。全部暂停/全部恢复只是批量状态变更，运行中队列可用 `POST /api/queues/{id}/entries` 追加条目。
  - 历史清理：`DELETE /api/queues/{id}` 只允许删除 done/cancelled 的队列（条目一并删除），活跃队列拒绝并提示先取消。前端默认只展示活跃队列，done/cancelled 收进「历史队列」折叠区，可在展开后单个删除。
  - 每条抓取成功后调用 `Product::record_crawl_result` 回填统计（中位数/均价/数量/最后爬取时间/回收价格）。`GATEWAY=mock` 下回收价用均价占位；`GATEWAY=webbridge` 走下方 AI 驱动抓取，回收价 = 中位数 × `RECYCLE_FACTOR`（默认 0.9）。
- **AI 驱动抓取**（`GATEWAY=webbridge`，实现在 `application/ai/crawl_agent_service.rs`）：队列条目由 ReAct agent 处理——AI 调 `xianyu_search`（经 `WebBridgeClient` 驱动本机真实浏览器带登录态搜闲鱼，导航到搜索页 → 等 SPA 渲染 → evaluate JS 提取候选，最多 30 条）→ AI 从候选中挑最多 8 个「描述最匹配、质量最高」的有效商品（剔除配件/求购/不相关/异常价）→ 调 `save_crawl_result` 提交，工具内算中位数/均价/回收价并写库（items + product 统计）。`save_crawl_result` 未被调用则条目记 failed；两次工具调用都落 `ai_tool_calls` 审计表。WebBridge 未启动/未登录/被风控时错误信息会指向浏览器标签组「闲鱼数据抓取」。
  - **用户自定义抓取提示词**（`AiSettingsService`，存 `app_settings` 表键 `crawl_custom_prompt`，`GET/PUT /api/ai/crawl-prompt` 读写，前端「AI → 抓取提示词」tab 编辑）：每次抓取读取最新值注入 agent system prompt（保存后下一轮即生效），可表达筛选与定价规则（如「CPU 类回收价打九折，显示器类打八折」）；`save_crawl_result` 支持可选 `recycle_factor` 参数（(0,1]，越界报错让 AI 重试），AI 按规则为商品选择系数，省略则用 `RECYCLE_FACTOR` 默认值。
- **AI 基础设施**（详细方案见 `docs/design-ai-module.md`）：
  - `AiGateway`/`AiTool` 端口在 `application/ports.rs`；真实实现 `RigAiGateway` 在 `infrastructure/ai_gateway.rs`，基于 `rig-core` 0.41 的 OpenAI 兼容端点（DeepSeek/千问/Kimi 等通用），手写 ReAct 工具循环。
  - AI 供应商配置入库管理（`ai_providers`）：前端「AI 接口管理」卡片可增删改查、设默认、测试连通性；密钥明文存本地 SQLite，响应掩码显示。优先级：**DB 默认配置 > `AI_API_KEY`/`AI_BASE_URL`/`AI_MODEL` 环境变量兜底**。
  - AI 工具可直接产生读写结果（完全自动，无人工确认）；每次工具执行落 `ai_tool_calls` 审计表（工具名/参数/结果或错误/耗时），前端「AI 工具调用记录」可回查。
  - 当前未实现具体 AI 用例（智能建标签、商品分类等），基础设施已就绪，后续用例来了只需在 `application/ai/` 加 service 并注册工具。

## 扩展约定

- 新业务（价格监控、推送等）：在 `domain/` 加实体、在 `application/` 加 service、在 `interfaces/` 加 handler + 路由，不要改已有代码的层次归属。
- 换基础设施（SQLite、真实闲鱼接口）：只改 `infrastructure/`。
- 将来拆多 crate/workspace：`domain/` 单独成 crate，依赖规则不变。
- 明确不做：不拆聚合根、不用 CQRS/事件溯源、不搞微服务（规模未到）。

## 代码约定

- 校验放值对象构造函数（`Keyword::new` / `PageRange::new`），非法输入返回 `DomainError::InvalidInput`。
- handler 只做「HTTP ↔ 应用层」翻译：解析参数 → 调 service → 包成 `ApiResponse`。业务逻辑不进 handler。
- 时间戳统一用 Unix 秒（`domain::crawl_task::now_unix()`），暂未引入 chrono。
- 修改本文件涉及的约定后，同步更新此 `AGENTS.md`。
