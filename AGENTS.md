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

## 架构（DDD-Lite，四层）

依赖规则：**外层可以依赖内层，内层绝不知道外层的存在**。`domain/` 不 import 任何外部框架（axum/reqwest/tokio 一律不出现）。跨层注入统一用 `Arc<dyn Trait>`。

```
src/
├── interfaces/          # 接口层：axum 路由、handler、dto（serde 注解只属于这里）
│   ├── mod.rs           #   build_router() + AppState（应用服务句柄）
│   ├── dto.rs           #   HTTP 请求/响应结构，与 domain 模型解耦
│   ├── crawl_handler.rs #   POST /api/crawl、GET /api/crawl/{id}
│   └── item_handler.rs  #   GET /api/items
├── application/         # 应用层：用例编排，不含业务规则
│   ├── ports.rs         #   端口 trait：XianYuGateway（防腐层）
│   ├── crawl_service.rs #   创建抓取任务 → 后台 tokio task 逐页抓取 → 落库
│   └── item_service.rs  #   商品列表查询
├── domain/              # 领域层：零外部依赖
│   ├── item.rs          #   Item 实体，Keyword/PageRange 值对象（含校验）
│   ├── crawl_task.rs    #   CrawlTask 实体：状态流转规则只写在实体方法里
│   ├── repository.rs    #   仓储端口 trait：ItemRepository / CrawlTaskRepository
│   └── error.rs         #   DomainError，全项目统一错误语义
└── infrastructure/      # 基础设施层：实现内层定义的 trait
    ├── config.rs        #   环境变量配置
    ├── xianyu_gateway.rs#   XianYuGateway 实现：Mock / Http（真实接口待实现）
    └── persistence/
        └── memory.rs    #   内存仓储（重启即失，骨架阶段用）
static/                  # 前端（原生 HTML/JS/CSS，无构建步骤，由 axum 托管）
```

## 核心设计决策

- **抓取是异步任务，不是同步请求**：`POST /api/crawl` 只创建任务并返回句柄，后台 tokio task 执行抓取，前端轮询 `GET /api/crawl/{id}`。防止多页抓取时 HTTP 超时。
- **任务状态机**：`Pending → Running → Done/Failed`。只有 Pending 能 `start()`，只有 Running 能 `finish()`；规则在 `CrawlTask` 实体方法里，不允许在别处直接改状态。
- **闲鱼网关是端口+实现（防腐层）**：`XianYuGateway` trait 定义在 `application/ports.rs`，实现（登录态、mtop 签名、解析）在 `infrastructure/xianyu_gateway.rs`。开发用 `GATEWAY=mock`。
- **仓储端口**：`ItemRepository` / `CrawlTaskRepository` trait 在 domain，当前实现是内存版。接 SQLite 时在 `infrastructure/persistence/` 新增实现并在 `main.rs` 替换，内层零改动。

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
