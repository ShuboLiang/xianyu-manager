# xianyu-manager

闲鱼爬虫与商品管理系统：Rust HTTP Web 服务（axum）+ 内置静态前端。

## 构建与运行

```bash
cargo build
cargo run        # 默认 http://127.0.0.1:3000
```

环境变量配置（见 `src/infrastructure/config.rs`）：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `HOST` | `127.0.0.1` | 监听地址 |
| `PORT` | `3000` | 监听端口 |
| `STATIC_DIR` | `static` | 前端静态文件目录 |
| `GATEWAY` | `mock` | `mock`=假数据网关，`http`=真实闲鱼接口（未实现） |
| `XIANYU_COOKIE` | - | `GATEWAY=http` 时的闲鱼登录态 Cookie |
| `DATABASE_PATH` | `data/xianyu.db` | SQLite 数据库文件路径（目录自动创建） |

## 架构（DDD-Lite，四层）

依赖规则：**外层可以依赖内层，内层绝不知道外层的存在**。`domain/` 不 import 任何外部框架（axum/reqwest/tokio 一律不出现）。跨层注入统一用 `Arc<dyn Trait>`。

```
src/
├── interfaces/          # 接口层：axum 路由、handler、dto（serde 注解只属于这里）
│   ├── mod.rs           #   build_router() + AppState（应用服务句柄）
│   ├── dto.rs           #   HTTP 请求/响应结构，与 domain 模型解耦
│   ├── crawl_handler.rs #   POST /api/crawl、GET /api/crawl/{id}
│   ├── item_handler.rs  #   GET /api/items
│   ├── tag_handler.rs   #   GET/POST /api/tags、GET/PUT/DELETE /api/tags/{id}
│   ├── product_handler.rs#  GET/POST /api/products、GET/PUT/DELETE /api/products/{id}
│   └── queue_handler.rs #   /api/queues 系列：预览、入队、暂停/恢复/取消、全部暂停/恢复、追加条目
├── application/         # 应用层：用例编排，不含业务规则
│   ├── ports.rs         #   端口 trait：XianYuGateway（防腐层）
│   ├── crawl_service.rs #   创建抓取任务 → 后台 tokio task 逐页抓取 → 落库
│   ├── item_service.rs  #   商品列表查询
│   ├── tag_service.rs   #   标签 CRUD（重名冲突校验、部分字段更新）
│   ├── product_service.rs#  待爬取商品 CRUD（重名校验、标签存在校验）
│   └── queue_service.rs #   抓取队列：选择器解析、入队去重、全局唯一 worker 串行消费
├── domain/              # 领域层：零外部依赖
│   ├── item.rs          #   Item 实体，Keyword/PageRange 值对象（含校验）
│   ├── crawl_task.rs    #   CrawlTask 实体：状态流转规则只写在实体方法里
│   ├── tag.rs           #   Tag 实体（TagName 值对象），enabled=false 的标签不参与抓取
│   ├── product.rs       #   Product 实体（待爬取商品）：名称/标签/备注 + 爬取统计字段
│   ├── crawl_queue.rs   #   CrawlQueue/CrawlEntry 实体、Selector 值对象、状态机
│   ├── repository.rs    #   仓储端口 trait：ItemRepository / CrawlTaskRepository / TagRepository / ProductRepository / QueueRepository
│   └── error.rs         #   DomainError，全项目统一错误语义
└── infrastructure/      # 基础设施层：实现内层定义的 trait
    ├── config.rs        #   环境变量配置
    ├── xianyu_gateway.rs#   XianYuGateway 实现：Mock / Http（真实接口待实现）
    └── persistence/
        ├── memory.rs    #   内存仓储：商品、抓取任务（重启即失）
        └── sqlite.rs    #   SQLite 仓储：标签、待爬取商品、抓取队列+条目（共享连接池，启动自动建表）
static/                  # 前端（原生 HTML/JS/CSS，无构建步骤，由 axum 托管）
```

## 核心设计决策

- **抓取是异步任务，不是同步请求**：`POST /api/crawl` 只创建任务并返回句柄，后台 tokio task 执行抓取，前端轮询 `GET /api/crawl/{id}`。防止多页抓取时 HTTP 超时。
- **任务状态机**：`Pending → Running → Done/Failed`。只有 Pending 能 `start()`，只有 Running 能 `finish()`；规则在 `CrawlTask` 实体方法里，不允许在别处直接改状态。
- **闲鱼网关是端口+实现（防腐层）**：`XianYuGateway` trait 定义在 `application/ports.rs`，实现（登录态、mtop 签名、解析）在 `infrastructure/xianyu_gateway.rs`。开发用 `GATEWAY=mock`。
- **仓储端口**：`ItemRepository` / `CrawlTaskRepository` / `TagRepository` trait 在 domain。商品和任务目前是内存实现；标签已落 SQLite（`infrastructure/persistence/sqlite.rs`，连接时自动建表）。换存储时新增实现并在 `main.rs` 替换，内层零改动。
- **标签管理**：标签（`domain/tag.rs`）管理「爬虫爬哪一类商品」，目前只含名称/启用状态/备注；抓取策略（关键词、频率、页数、过滤规则等）后续挂在标签上扩展。`enabled=false` 的标签届时不参与抓取。标签名全局唯一，冲突返回 `DomainError::Conflict`。
- **待爬取商品管理**：商品（`domain/product.rs`）管理「要爬哪些商品」。基础信息：名称（唯一，冲突返回 `Conflict`）、标签（**多对多**，`tag_ids: Vec<i64>`，默认空=无标签；存 `product_tags` 关联表，删除商品或标签时外键 `ON DELETE CASCADE` 自动清理关联）、备注。统计字段（中位数/均价/爬取数量/最后爬取时间/回收价格）只由爬取结果写入（`Product::record_crawl_result`），未爬取时为 null。更新接口的 `tag_ids`：不传=不修改，空数组=清空全部标签，非空数组=整体替换。
- **删除语义**：数据库层一律 `CASCADE` 兜底（删标签/删商品只清关联，另一方不受影响）；交互层做影响提示——`GET /api/tags/{id}/products` 返回使用该标签的商品，前端删除标签前在确认框中列出受影响商品。不做「阻止删除」。
- **抓取队列**（详细方案见 `docs/design-crawl-queue.md`）：
  - 队列 = 商品 id 快照（`crawl_entries`），入队后改标签/规则不影响本队列；删除商品时其条目由 worker 标记为 `skipped`，不阻塞队列。
  - 入队方式二选一：`selector`（`tag_all`/`tag_any`/`tag_exclude`/`stale_days`，维度间 AND）或 `product_ids`（手动勾选）；入队前可 `POST /api/queues/preview` 预览命中与跳过。
  - **全局去重**：同一商品只允许存在于一个活跃（waiting/running/paused）队列中，重复者入队响应的 `skipped` 列出；done/cancelled 队列不占位。
  - **全局唯一 worker**：`QueueService::start_worker` 在 `main.rs` 启动时拉起，串行消费，任意时刻最多一个 running 队列；running 结束/暂停后最早的 waiting 自动顶上。条目间隔 `interval_secs`（睡眠切成 1 秒小片段以便及时响应暂停）。
  - 队列状态机：`waiting → running → paused/done/cancelled`，`paused → waiting`（恢复=重新排队，无差别恢复）；暂停是**优雅暂停**——当前条目跑完才停。全部暂停/全部恢复只是批量状态变更，运行中队列可用 `POST /api/queues/{id}/entries` 追加条目。
  - 历史清理：`DELETE /api/queues/{id}` 只允许删除 done/cancelled 的队列（条目一并删除），活跃队列拒绝并提示先取消。前端默认只展示活跃队列，done/cancelled 收进「历史队列」折叠区，可在展开后单个删除。
  - 每条抓取成功后调用 `Product::record_crawl_result` 回填统计（中位数/均价/数量/最后爬取时间/回收价格）。真实爬虫未实现，`GATEWAY=mock` 下回收价用均价占位。

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
