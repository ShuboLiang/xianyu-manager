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
| `AI_CRAWL_MODE` | `direct`                    | AI 抓取实现兜底：`direct`=单轮调用（省 token）；`agent`=ReAct 工具循环。**DB 设置优先**（app_settings 键 `ai_crawl_mode`，`GET/PUT /api/ai/crawl-mode` 读写，前端「AI → 抓取提示词」tab 顶部切换，下一轮抓取生效无需重启） |
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
│   ├── product_handler.rs#  GET/POST /api/products（列表分页 + 服务端排序）、GET/PUT/DELETE /api/products/{id}、GET /api/products/{id}/latest-items（最后一轮抓取明细）、GET /api/products/price-trend（趋势图）、GET /api/products/export（xlsx 导出）、POST /api/products/batch（批量导入）、POST /api/products/batch-delete(/preview)（按标签批删）
│   ├── queue_handler.rs #   /api/queues 系列：预览、入队、暂停/恢复/取消、全部暂停/恢复、追加条目、改名、历史清理 purge
│   ├── ai_handler.rs    #   /api/ai 系列：provider 增删改查、设为默认、连通性测试、工具调用审计（分页 + 工具名/成败筛选、保留期清理 purge）、GET/PUT /api/ai/crawl-prompt（自定义抓取提示词）、GET/PUT /api/ai/crawl-mode（抓取模式切换）、POST /api/ai/classify-products（同步打标签）、/api/ai/classify-tasks 系列（异步任务：创建/查询/取消）、GET /api/ai/tools（管理 Agent 工具清单）、POST /api/ai/chat（通用管理助手）
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
│   ├── ai_tool_call_service.rs # AI 工具调用审计：分页筛选查询（工具名/成败）+ 保留期清理（PurgeCriteria 二选一校验）
│   ├── trend_service.rs #   价格趋势计算（按商品聚合 items 成 PriceTrendSeries）
│   ├── cancel_token.rs  #   共享取消令牌存储（watch 信号，异步分类任务取消用）
│   ├── ai/            #   AI 用例：classify_service（自动打标签）/ crawl_agent_service（ReAct 抓取）/ crawl_direct_service（单轮调用抓取，默认）/ crawl_shared（ProductCrawler 端口 + 统计落库共享）/ crawl_switch（运行时按 app_settings 切换两种抓取实现）/ admin_tools（通用管理 Agent 工具集：AdminToolsService + 19 个 AiTool，见下）
│   └── stats_service.rs #   KPI 概览统计（商品总数 / 24h 抓取 / 最后爬取时间）
├── domain/              # 领域层：零外部依赖
│   ├── item.rs          #   Item 实体，Keyword/PageRange 值对象（含校验）
│   ├── crawl_task.rs    #   CrawlTask 实体：状态流转规则只写在实体方法里（Pending→Running→Done/Failed，含 fail 守卫）；任务 ID 为 uuid crate 的 UUIDv4
│   ├── tag.rs           #   Tag 实体（TagName 值对象），enabled=false 的标签不参与抓取
│   ├── product.rs       #   Product 实体（待爬取商品）：名称/标签/备注 + 爬取统计字段
│   ├── crawl_queue.rs   #   CrawlQueue/CrawlEntry 实体、Selector 值对象、状态机（队列与条目的状态流转都只能走实体方法）
│   ├── ai_provider.rs   #   AiProvider 实体（OpenAI 兼容端点配置）+ ProviderName/BaseUrl/ModelName 值对象
│   ├── ai_classify_task.rs # AiClassifyTask 实体：自动打标签异步任务，状态机同 CrawlTask（内存仓储，重启即失）
│   ├── ai_tool_call.rs  #   AiToolCall 审计实体
│   ├── price_trend.rs   #   PriceTrendPoint/PriceTrendSeries 只读结构（价格趋势图数据，无行为）
│   ├── repository.rs    #   仓储端口 trait：ItemRepository / CrawlTaskRepository / TagRepository / ProductRepository / QueueRepository / AiProviderRepository / AiClassifyTaskRepository / AiToolCallRepository / SettingsRepository（KV 设置）
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
web/                     # 前端源码：React 19 + TS + Vite 7 + antd v5 + @tanstack/react-query
├── src/types/generated/ #   ts-rs 从 dto.rs 自动生成的类型（cargo test export_bindings，勿手改）
├── src/types/api.ts     #   类型的友好别名 re-export + 手写 ApiResponse<T> 包装
├── src/lib/api.ts       #   fetch 封装：解包 ApiResponse<T>，code!==0 抛错；fmtPrice/fmtTime 格式化
├── src/lib/queries.ts   #   全局共享数据的 react-query hooks（tags/queues/stats/health；queues 自动 2s 轮询）
├── src/lib/queue.tsx    #   QueueContext：队列状态 + 入队/追加/间隔，AppShell 提供实现，任意页面可用
├── src/AppShell.tsx     #   布局壳：Layout.Sider 导航 + Header（队列状态指示/健康徽标/主题切换）+ Outlet
├── src/components/      #   共享小组件（PageHeader 页头、AiChat AI 助手聊天面板）
└── src/pages/           #   六个页面：OverviewPage（KPI + QueuesPanel）/ ProductsPage / TagsPage / ItemsPage / TrendsPage / AiPage
```

前端约定：

- `web/vite.config.ts`：dev 端口 5173（避让后端 3000），`/api` 代理到 `127.0.0.1:3000`；`build.outDir` 指向 `../static` 且 `emptyOutDir`，构建即覆盖旧产物；antd/recharts/react 经 `manualChunks` 独立分包。
- **UI 体系是 antd v5，不再使用 Tailwind/shadcn**（2026-08 重写）：主题 token 集中在 `App.tsx` 的 `ConfigProvider`（琥珀主色 `#d97706` 延续闲鱼黄、全局 `compactAlgorithm` 紧凑密度、dark/light `algorithm` 切换）；深浅色状态在 `lib/theme-mode.ts`（localStorage 持久化）；样式优先用 antd 组件与 token，少量自定义样式写 `src/index.css`（如 `.num` 等宽数字字体）。反馈一律走 `AntApp.useApp()` 的 `message`/`modal`（不用 antd 静态函数）。
- 前端用 `react-router` 的 **HashRouter**（避免 axum 静态托管需要路由回退）：六个页面 = 概览（KPI + 队列）/ 商品管理 / 标签管理 / 抓取数据 / 价格趋势 / AI 配置，路由声明在 `App.tsx`，布局壳是 `AppShell.tsx`（antd Layout：Sider 导航 + 48px Header）。Header 常驻队列运行指示（运行中队列进度/排队数），任何页面可感知系统在跑。
- **服务端状态一律走 @tanstack/react-query**，不再在 `App.tsx` 手写 useState+useEffect 加载：共享数据 hooks 在 `lib/queries.ts`；页面级分页查询（products/items/aiCalls）以查询条件对象作 queryKey 定义在各页面内（`placeholderData: keepPreviousData` 防翻页闪烁）；变更操作后 `invalidateQueries` 对应 key。
- 队列轮询逻辑在 `lib/queries.ts::useQueues`：有 waiting/running 队列时 `refetchInterval` 2 秒自刷新；刚全部结束的判定在 `AppShell.tsx`（useEffect 比对前后状态），届时补刷 products/items/stats 三个 queryKey。
- 跨页面的队列操作（商品页入队、概览页追加条目、appendTarget/intervalSecs 共享状态）走 `lib/queue.tsx` 的 `QueueContext`，由 `AppShell` 提供实现（含入队成功/失败的 message 提示与 queues 失效）。
- **前后端类型自动同步（ts-rs）**：`interfaces/dto.rs` 的 DTO 全部 `#[derive(TS)]` + `#[ts(export)]`，运行 `cargo test export_bindings` 会把 TS 类型写入 `web/src/types/generated/`（导出目录配置在 `.cargo/config.toml` 的 `TS_RS_EXPORT_DIR`）。前端 `web/src/types/api.ts` 只做别名 re-export（`TagResponse`→`Tag` 等）与 `ApiResponse<T>` 手写包装，**不再手工维护字段**。改 dto.rs 后必须重跑导出 + `npm run build`。
  - 新增 DTO 的 `i64`/`u64`/`Vec<i64>` 字段必须标 `#[ts(type = "number")]` / `#[ts(type = "Array<number>")]`（ts-rs 默认映射 bigint，与 JSON 实际序列化不符）。
  - `ApiResponse<T>` 泛型不经 ts-rs 导出；`QueueResponse.status` 在 Rust 是 `String`，前端 `api.ts` 里收窄为 `QueueStatus` 联合类型。

## 核心设计决策

- **抓取是异步任务，不是同步请求**：`POST /api/crawl` 只创建任务并返回句柄，后台 tokio task 执行抓取，前端轮询 `GET /api/crawl/{id}`。防止多页抓取时 HTTP 超时。
- **任务状态机**：`Pending → Running → Done/Failed`。只有 Pending 能 `start()`，只有 Running 能 `finish()`/`fail()`；规则在 `CrawlTask` 实体方法里，不允许在别处直接改状态。队列条目同理：只有 Pending 能 `start()`，只有 Running 能 `done()`/`fail()`/`skip()`（`CrawlEntry` 实体方法，worker 不直接改字段）。
- **闲鱼网关是端口+实现（防腐层）**：`XianYuGateway` trait 定义在 `application/ports.rs`，实现（登录态、mtop 签名、解析）在 `infrastructure/xianyu_gateway.rs`。开发用 `GATEWAY=mock`。
- **仓储端口**：`ItemRepository` / `CrawlTaskRepository` / `TagRepository` trait 在 domain。抓取商品数据（items 表，id=详情页 URL，重复抓取 INSERT OR REPLACE 覆盖）、标签、商品、队列、AI 配置/审计均已落 SQLite（`infrastructure/persistence/sqlite.rs`，连接时自动建表）；抓取任务与 AI 分类任务仍是内存实现（重启即失，可接受）。换存储时新增实现并在 `main.rs` 替换，内层零改动。
- **标签管理**：标签（`domain/tag.rs`）管理「爬虫爬哪一类商品」，目前只含名称/启用状态/备注；抓取策略（关键词、频率、页数、过滤规则等）后续挂在标签上扩展。`enabled=false` 的标签届时不参与抓取。标签名全局唯一，冲突返回 `DomainError::Conflict`。
- **待爬取商品管理**：商品（`domain/product.rs`）管理「要爬哪些商品」。基础信息：名称（唯一，冲突返回 `Conflict`）、标签（**多对多**，`tag_ids: Vec<i64>`，默认空=无标签；存 `product_tags` 关联表，删除商品或标签时外键 `ON DELETE CASCADE` 自动清理关联）、备注。支持**批量导入**（`POST /api/products/batch`，每行一个名称，可指定统一标签，返回成功/跳过明细）与 **xlsx 导出**（`GET /api/products/export`，rust_xlsxwriter 生成）。统计字段（中位数/均价/常见价位/爬取数量/最后爬取时间/回收价格）由爬取结果写入（`Product::record_crawl_result`），未爬取时为 null；**常见价位是自适应档宽的分档众数**（`domain/product.rs::mode_price`：档宽按价格量级——<100 十元档、<1000 五十元档、<10000 百元档、≥10000 五百元档，规则在 `mode_bucket_width`，前端/xlsx 按同一规则从下界推出区间上界；取商品数最多的档，并列取较低档——原始众数对连续价格没有意义；落库存档位下界，展示为区间「¥90–100」「¥1200–1300」），与统计字段一起落库、支持列头排序、xlsx 导出含该列；**回收价例外地允许手动设置/清空**（更新接口 `recycle_price`：不传=不修改，null=清空，数值=设定，校验在 `Product::set_recycle_price`，下一轮爬取会覆盖手动值；前端商品表格回收价单元格行内编辑）。更新接口的 `tag_ids`：不传=不修改，空数组=清空全部标签，非空数组=整体替换。
- **删除语义**：数据库层一律 `CASCADE` 兜底（删标签/删商品只清关联，另一方不受影响）；交互层做影响提示——`GET /api/tags/{id}/products` 返回使用该标签的商品，前端删除标签前在确认框中列出受影响商品。不做「阻止删除」。
  - **删除商品不删抓取历史**：items 主键是详情页 URL、跨商品共享覆盖，不专属某个商品，所以删商品（单个或批量）时 items 一律保留，仅由 `ItemRepository::detach_product` 把 `items.product_id` 置 NULL 解除归属（避免悬空引用，抓取数据页商品名显示「-」）。
  - **按标签批量删除商品**：`POST /api/products/batch-delete/preview` 预览命中商品与活跃队列占用数（仅警告不阻止），`POST /api/products/batch-delete` 执行（入参 `{tag_id}`，标签本身保留）；活跃队列中的条目沿用 worker 标记 `skipped` 兜底。前端入口在标签管理页每行的「删除商品」。
  - **抓取数据（items）删除**：`DELETE /api/items/{id}` 单条删除（id 是 TEXT，前端需 `encodeURIComponent`）；`POST /api/items/batch-delete/preview` + `POST /api/items/batch-delete` 按搜索条件批量删除，`search` 与列表搜索同一 WHERE 语义（标题或所属商品名模糊，删除侧用子查询表达商品名条件，SQLite DELETE 不支持 JOIN），空 search = 清空全部；`POST /api/items/batch-delete-ids(/preview)` 勾选批量删除（按 id 列表，preview 返回实际存在数 + 前 10 条标题样本）。删 items 不回退商品已算统计，但价格趋势图对应数据点会消失（确认框文案需写明）。预览/确认交互与商品批删一致：数量 + 前 10 条样本 + 「等 N 条」。
  - **勾选批量删除商品**：`POST /api/products/batch-delete-ids(/preview)` 按 id 列表删除，preview 返回实际存在数 + 前 10 条名称样本 + 活跃队列占用数（仅提示不阻止）；删除语义同单个删除（items 保留、detach_product 解除归属）。前端批量操作一律走专门的批量接口，不允许前端 for 循环调单条接口。
- **列表分页**：`/api/items`、`/api/products`、`/api/ai/tool-calls` 三个列表接口服务端分页，统一 `page`（从 1 起）/ `page_size`（默认 20，clamp 1..=100，钳制逻辑在 `item_handler::normalize_page`），响应为 `PageResponse<T> { items, total, page, page_size }`（`PageResponse<T>` 泛型不经 ts-rs 导出，前端 `api.ts` 手写）。商品列表支持服务端排序（`sort_by`/`sort_dir`，排序列白名单枚举 `ProductSortColumn` 在 `domain/repository.rs`，SQL 列名映射在 `sqlite.rs::product_sort_column_sql`，SQL 空值沉底）；前端列头排序只是改查询条件回第 1 页。items 和 products 列表都支持 `tag_id` 标签筛选（items 侧语义 = 记录所属商品挂在该标签下，SQL 为 `EXISTS` 子查询，`product_id` 为 NULL 的记录永不命中；注意该筛选只作用于列表展示，items 批删的 `search` 语义不含标签条件）。tags 和 queues 不分页（标签是全局选项源，队列轮询需全量活跃视图）。KPI 概览不从列表数据推导，由 `GET /api/stats` 提供（product_count / crawled_today=滚动 24h 窗口 / last_crawled_at）。
- **抓取队列**（详细方案见 `docs/design-crawl-queue.md`）：
  - 队列 = 商品 id 快照（`crawl_entries`），入队后改标签/规则不影响本队列；删除商品时其条目由 worker 标记为 `skipped`，不阻塞队列。
  - 入队方式二选一：`selector`（`tag_all`/`tag_any`/`tag_exclude`/`stale_days`，维度间 AND）或 `product_ids`（手动勾选）；入队前可 `POST /api/queues/preview` 预览命中与跳过。
  - **全局去重**：同一商品只允许存在于一个活跃（waiting/running/paused）队列中，重复者入队响应的 `skipped` 列出；done/cancelled 队列不占位。
  - **全局唯一 worker**：`QueueService::start_worker` 在 `main.rs` 启动时拉起，串行消费，任意时刻最多一个 running 队列；running 结束/暂停后最早的 waiting 自动顶上。条目间隔 `interval_secs`（睡眠切成 1 秒小片段以便及时响应暂停）。
  - 队列状态机：`waiting → running → paused/done/cancelled`，`paused → waiting`（恢复=重新排队，无差别恢复）；暂停是**优雅暂停**——当前条目跑完才停。全部暂停/全部恢复只是批量状态变更，运行中队列可用 `POST /api/queues/{id}/entries` 追加条目。
  - **队列名称**：`CrawlQueue.name` 在入队时由圈选条件自动生成（选择器 → 「标签：A＋B ∧ 7 天未爬」，手动勾选 → 「手动勾选」，生成逻辑在 `queue_service.rs::target_summary`，件数不写进名称）；追加条目时新条件以「＋」并入名称（`CrawlQueue::append_condition`，超 `QUEUE_NAME_MAX_LEN`=40 字符截断），用户手动改名（`PUT /api/queues/{id}/name` → `CrawlQueue::rename`，置 `name_custom=true`）后不再自动拼条件；老库经启动迁移补 `name`/`name_custom` 列（name 默认为空串，前端回退显示「队列 #id」）。前端所有出现队列的地方（面板列、运行卡片、追加目标、Header 指示）一律显示名称。
  - 历史清理：`DELETE /api/queues/{id}` 只允许删除 done/cancelled 的队列（条目一并删除），活跃队列拒绝并提示先取消。前端默认只展示活跃队列，done/cancelled 收进「历史队列」折叠区，可在展开后单个删除；也可按条件批量清理——`POST /api/queues/purge(/preview)`，条件二选一（`QueuePurgeCriteria::new` 校验恰填一个）：`before_days`（清 N 天前结束的，0=清空全部历史）或 `keep_latest`（仅保留最近结束的 N 条）；只作用于 done/cancelled，preview 返回将删的队列数与条目总数，前端「清理历史」弹窗走 preview/confirm 模式。
  - 每条抓取成功后调用 `Product::record_crawl_result` 回填统计（中位数/均价/常见价位/数量/最后爬取时间/回收价格）。`GATEWAY=mock` 下回收价用均价占位；`GATEWAY=webbridge` 走下方 AI 驱动抓取，回收价 = 中位数 × `RECYCLE_FACTOR`（默认 0.9）。
- **AI 驱动抓取**（`GATEWAY=webbridge`）：队列条目由 `ProductCrawler`（端口在 `application/ai/crawl_shared.rs`，`QueueService` 只依赖该 trait）处理。两种实现同时构造，由 `SwitchableCrawler`（`crawl_switch.rs`）**每次抓取实时读取** `app_settings` 的 `ai_crawl_mode` 决定走哪条（DB 设置 > `AI_CRAWL_MODE` 环境变量兜底，前端「AI → 抓取提示词」tab 切换后下一轮生效）；统计计算与落库（中位数/均价/常见价位/回收价 = 中位数 × `RECYCLE_FACTOR`，默认 0.9）共享 `crawl_shared::finalize_crawl`：
  - **`direct` 单轮调用（默认，`crawl_direct_service.rs`，省 token）**：Rust 直接用商品名搜索 → 候选 < 5 条才补一次袖珍调用换词重搜 → 一次 completion 让 AI 返回选中序号 JSON（候选以「序号. 标题 ¥价格 · 卖家」进 prompt，**URL 不进 prompt**，AI 回序号、Rust 按序号取回完整字段，候选上限 20）→ Rust 校验序号、算统计、落库。解析失败重试一次；非法 recycle_factor 兜底默认系数。LLM 调用 1 次（兜底 2 次），相比 agent 路径省约 70% token。无真实工具调用，但各步骤**手写审计记录**落 `ai_tool_calls`（`xianyu_search` / `refine_search_keyword` / `crawl_select` / `save_crawl_result`，尽力而为不阻塞抓取），回查形态与 agent 路径一致。
  - **`agent` ReAct 循环（旧路径，保留，`crawl_agent_service.rs`）**：AI 调 `xianyu_search`（经 `WebBridgeClient` 驱动本机真实浏览器带登录态搜闲鱼，导航到搜索页 → 等 SPA 渲染 → evaluate JS 提取候选，最多 30 条）→ AI 从候选中挑最多 8 个有效商品（剔除配件/求购/不相关/异常价）→ 调 `save_crawl_result` 提交。`save_crawl_result` 未被调用则条目记 failed；两次工具调用都落 `ai_tool_calls` 审计表。
  - 公共部分：WebBridge 未启动/未登录/被风控时错误信息会指向浏览器标签组「闲鱼数据抓取」；**用户自定义抓取提示词**（`AiSettingsService`，存 `app_settings` 表键 `crawl_custom_prompt`，`GET/PUT /api/ai/crawl-prompt` 读写，前端「AI → 抓取提示词」tab 编辑）每次抓取读取最新值注入提示词（保存后下一轮即生效），可表达筛选与定价规则（如「CPU 类回收价打九折」）；AI 按规则给出系数（agent 走 `save_crawl_result` 的 `recycle_factor` 参数（(0,1]，越界报错让 AI 重试），direct 走 JSON 的 `recycle_factor` 字段），省略则用 `RECYCLE_FACTOR` 默认值。
- **AI 基础设施**（详细方案见 `docs/design-ai-module.md`）：
  - `AiGateway`/`AiTool` 端口在 `application/ports.rs`；真实实现 `RigAiGateway` 在 `infrastructure/ai_gateway.rs`，基于 `rig-core` 0.41 的 OpenAI 兼容端点（DeepSeek/千问/Kimi 等通用），手写 ReAct 工具循环。
  - AI 供应商配置入库管理（`ai_providers`）：前端「AI 接口管理」卡片可增删改查、设默认、测试连通性；密钥明文存本地 SQLite，响应掩码显示。优先级：**DB 默认配置 > `AI_API_KEY`/`AI_BASE_URL`/`AI_MODEL` 环境变量兜底**。
  - **额外请求参数（extra_params）**：provider 上的可选 JSON 对象（`ExtraParams` 值对象校验必须是合法 JSON 对象，更新语义同 api_key——None=不修改/空串=清空/值=替换；老库启动时 ALTER 补列），请求时经 rig `additional_params_opt` 原样合并进请求体（`complete` 与 `run_agent` 都生效）。端点私有参数不写死代码：DeepSeek 关思考 `{"thinking": {"type": "disabled"}}`（V4 默认开启 thinking 且 effort=high，思考链按输出计费，简单任务建议关掉）、调推理强度 `{"reasoning_effort": "low"}`、千问 `{"enable_thinking": false}` 等。前端只暴露「思考模式」开关 + 思考等级（低/高/最高，默认高=供应商默认不带参数；千问系 base_url 自动换 enable_thinking 格式并隐藏等级选择），保存/回显由 `parseThinkingParams`/`buildThinkingParams` 互转；表格「思考模式」列显示「关思考 / 开·默认 / 思考·低 等」。
  - AI 工具可直接产生读写结果（完全自动，无人工确认）；每次工具执行落 `ai_tool_calls` 审计表（工具名/参数/结果或错误/耗时），前端「AI 工具调用记录」可回查（按工具名/成败筛选）。审计记录只增不改，**不做单条删除**；膨胀控制走保留期清理：`POST /api/ai/tool-calls/purge(/preview)`，条件二选一——`before_days`（删 N 天前，0=清空）或 `keep_latest`（仅保留最新 N 条），交互沿用 preview/confirm 批删模式。
  - **Token 用量审计**：`ai_tool_calls` 另有 `input_tokens`/`output_tokens`/`cached_input_tokens` 三列（可空，纯工具行与供应商未上报时为 NULL；老库启动时 ALTER 迁移补齐）。`AiGateway::complete` 返回 `AiCompletion { text, usage }`（`TokenUsage` 定义在 `application/ports.rs`，供应商 usage 全 0 视为未上报→None）；direct 路径的 `crawl_select`/`refine_search_keyword` 行带用量，agent 路径每轮 LLM 调用落一行 `llm_call` 审计（参数只记轮次，结果记回复长度与本轮工具名）。前端调用记录表格「Token 入/出」列展示，缓存命中附注。
  - **AI 自动打标签用例**（`application/ai/classify_service.rs`，详细方案见 `docs/design-batch-import-ai-classify.md`）：传入商品 id 列表，AI 调 `list_tags` 等工具为商品匹配标签并写回。两条路径：`POST /api/ai/classify-products` 同步（单次上限 50 个）与 `POST /api/ai/classify-tasks` 异步任务（每批 50 个，内存仓储，可查进度、可取消——取消信号走 `cancel_token.rs` 的 watch 令牌）。后续新用例来了只需在 `application/ai/` 加 service 并注册工具。
  - **通用管理 Agent 工具集**（`application/ai/admin_tools.rs`，**19 个 AiTool**）：把后台各接口的能力直接暴露成工具，供 `POST /api/ai/chat`（自然语言 → agent 自主调工具查改数据）与外部智能体使用。工具列表：商品 `list_products`/`get_product`/`create_product`/`update_product`/`delete_product`/`batch_create_products`，标签 `list_tags`/`create_tag`/`update_tag`/`delete_tag`，抓取记录 `list_items`，统计 `get_stats`，队列 `list_queues`/`get_queue`/`enqueue`/`pause_queue`/`resume_queue`/`cancel_queue`，趋势 `get_price_trend`。
    - 设计：工具**直接调 application 层 service**（复用业务规则：重名校验、标签存在校验、队列去重等），不走 HTTP；读工具收敛结果（分页/字段裁剪）防上下文淹没；删除类工具 description 要求先确认。
    - `AdminToolsService::tools()` 集中注册；`GET /api/ai/tools` 返回 name/description/参数 Schema（JSON，供外部智能体动态注册）；`POST /api/ai/chat` 入参 `{message}`，走 `run_agent` 循环（max_rounds=12），所有工具调用自动落 `ai_tool_calls` 审计。前端「AI 配置 → AI 助手」tab（`web/src/components/AiChat.tsx`）内置对话界面：自然语言指令 → 调 `POST /api/ai/chat` → 展示 agent 回答（ReAct 循环在后端跑，前端等最终结果）；未配置 AI 时给出引导。
    - 新增工具约定：在 `admin_tools.rs` 里加一个实现 `AiTool` 的 struct（持有对应 service 的 `Arc`），在 `AdminToolsService::tools()` 注册即可；写工具（尤其删除/入队）在 description 里写清副作用。有副作用写操作目前无人工确认（与 AI 抓取同策略），如需加确认机制再迭代。

## 扩展约定

- 新业务（价格监控、推送等）：在 `domain/` 加实体、在 `application/` 加 service、在 `interfaces/` 加 handler + 路由，不要改已有代码的层次归属。
- 换基础设施（SQLite、真实闲鱼接口）：只改 `infrastructure/`。
- 将来拆多 crate/workspace：`domain/` 单独成 crate，依赖规则不变。
- 明确不做：不拆聚合根、不用 CQRS/事件溯源、不搞微服务（规模未到）。

## 代码约定

- 校验放值对象构造函数（`Keyword::new` / `PageRange::new` / `ProductName::new` / `TagName::new` / `ProviderName::new` / `BaseUrl::new` / `ModelName::new`），非法输入返回 `DomainError::InvalidInput`；不做事后校验，也不用静默钳制（如 `timeout_secs.max(1)`）代替报错。
- 领域层不出现 SQL 细节：`ProductSortColumn` 等枚举只定义白名单语义，「枚举 → SQL 列名」的映射写在 `infrastructure/persistence/sqlite.rs`。
- 用户输入（搜索词、筛选值）一律走 bind 参数，不拼进 SQL；LIKE 模糊匹配用 `like_escape` 转义通配符（`\ % _`）+ `ESCAPE '\'`。多步写操作（商品行 + 标签关联等）必须包在 sqlx 事务里，不留半状态。
- handler 只做「HTTP ↔ 应用层」翻译：解析参数 → 调 service → 包成 `ApiResponse`。业务逻辑不进 handler。
- 时间戳统一用 Unix 秒（`domain::crawl_task::now_unix()`），暂未引入 chrono。
- 修改本文件涉及的约定后，同步更新此 `AGENTS.md`。
