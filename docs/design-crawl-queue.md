# 抓取队列设计方案（待审查）

> 状态：待审查。审查通过后按本文档实施。
> 范围：队列系统完整落地；真实爬虫**不实现**，worker 用 Mock/日志代替。

## 1. 背景与目标

- 闲鱼有反爬机制，抓取必须**串行**，请求间带间隔+随机抖动。
- 手动逐个商品发起抓取太麻烦，需要**按规则批量圈选**商品加入队列。
- 队列执行可能持续很久，需要可**暂停/恢复/取消**，进度可查，重启不丢。
- 真实爬虫方法（`HttpXianYuGateway`）本期留空，worker 用 Mock 数据 + 日志打印验证全链路。

## 2. 核心概念

### 2.1 选择器（Selector）——入队规则

各维度之间是 AND，标签内部支持三种关系：

| 字段 | 含义 | 对应需求 |
|---|---|---|
| `tag_all: [i64]` | 必须全部包含 | 「标签满足 a 和 b」 |
| `tag_any: [i64]` | 至少包含其一 | 「标签是 a」「a 或 b」 |
| `tag_exclude: [i64]` | 不能包含 | 「a 或 b 但不包含 c」 |
| `stale_days: u32?` | 最后爬取时间距今 ≥ N 天（从未爬过也算） | 「七天没爬的都加入」 |

示例：
- 标签 a → `{tag_any: [a]}`
- a 且 b → `{tag_all: [a, b]}`
- (a 或 b) 非 c → `{tag_any: [a, b], tag_exclude: [c]}`
- 七天未爬 → `{stale_days: 7}`
- 七天未爬 且 标签 a → `{stale_days: 7, tag_any: [a]}`

约束与语义：
- 选择器不能为空（至少一个条件），防止误操作全量入队。
- `stale_days` 是**可选**的：不传则完全不做时间过滤。例如 `{tag_any:[a]}` = 所有标签 a 的商品，不管多久没爬。
- **各维度之间固定是 AND（交集）**，不支持跨维度 OR（并集）。并集需求用「建多个队列」替代：分别按标签 a、按七天未爬各建一个队列，全局去重会自动处理重叠商品，效果等价且结构简单。
- 「从未爬过也算 stale」本期写死；将来如有需要可加 `include_never_crawled: bool`（默认 true），本期不做。

### 2.2 手动指定商品（无规则入队）

入队/预览接口除了选择器，还接受**显式商品 id 列表** `product_ids: [i64]`，用于「只更新单个商品」或「手动勾选的几个商品」：

- 两种模式二选一：传 `product_ids` 就不要求选择器非空；传选择器就走规则匹配。
- 手动模式同样校验商品存在、同样走全局去重（已在队列的会被跳过并在响应里说明）。
- 前端配套：商品列表每行加勾选框 + 「选中加入队列」按钮；每行加「抓取」快捷按钮（单商品入队）。

### 2.3 队列 = 商品 id 快照

点「加入队列」那一刻，选择器（或手动 id 列表）求值成一份**商品 id 列表**落库为队列条目。此后队列与标签无关：

- 入队后**删除标签** → 无影响（队列里没有标签）；
- 入队后**删除商品** → worker 执行到该条目时发现商品不存在，标记 `skipped` 跳过，不影响其他条目；
- 入队后**商品改名** → 条目只存 id，执行时现查最新名称搜索。

### 2.4 状态机

**同一时刻最多一个队列处于 running（全局串行）**，因此队列状态为：

`waiting（排队中）→ running → paused / done / cancelled`，`paused → waiting（恢复时若已有队列在跑则重新排队）`

条目：`pending → running → done / failed / skipped`。

- **创建队列**：当前无 running 队列则直接 `running`；否则 `waiting`，等前面的队列暂停/跑完/取消后按创建顺序自动顶上。
- **暂停/恢复**：暂停释放执行位（下一个 waiting 队列自动开始）；恢复时若执行位空闲则直接 `running`，否则转入 `waiting` 排队，不需要重新入队。
- **取消**：终止态（waiting/running/paused 均可取消），条目记录保留可查，不删除数据；取消 running 队列同样释放执行位。
- 追加条目：waiting/running/paused 都允许。

### 2.5 全部暂停 / 全部恢复（批量状态操作）

没有旁路的调度器开关，就是两条批量状态变更：

- **全部暂停**（`POST /api/queues/pause-all`）：所有 running/waiting 队列 → `paused`。没有 running 队列，调度自然停止。
- **全部恢复**（`POST /api/queues/resume-all`）：所有 paused 队列 → `waiting`（不区分手动暂停还是批量打停），随后按创建顺序自动顶起一个 running。

取舍说明（明确接受）：手动暂停的队列也会被「全部恢复」拉起；不希望某队列再跑就用「取消」而不是「暂停」。

## 3. 去重与预览（解决「不知道加了什么没加」）

- **全局去重**：商品只要在任何未结束队列（waiting/running/paused）中是 pending/running，再次圈选时自动跳过，不会重复入队。
- **入队前预览**：`POST /api/queues/preview` 用同样规则求值但不落库，返回「匹配 N 个，其中 M 个已在队列将被跳过，实际新增 K 个」+ 名单。前端确认后才真正入队。
- 商品列表后续可加「队列状态」列（本期可不做）。

## 4. 数据模型（SQLite 新表）

```sql
CREATE TABLE crawl_queues (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    status        TEXT NOT NULL,        -- waiting/running/paused/done/cancelled
    interval_secs INTEGER NOT NULL,     -- 基础间隔（秒），执行时叠加随机抖动
    created_at    INTEGER NOT NULL,
    finished_at   INTEGER
);

CREATE TABLE crawl_entries (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    queue_id   INTEGER NOT NULL,
    product_id INTEGER NOT NULL,        -- 故意不加外键：商品删除后条目保留 → skipped
    status     TEXT NOT NULL,           -- pending/running/done/failed/skipped
    error      TEXT,
    crawled_at INTEGER
);
```

## 5. API

| 方法 | 路径 | 说明 |
|---|---|---|
| POST | `/api/queues/preview` | 预览（选择器或 `product_ids` 二选一）：返回将新增/将跳过的商品名单，不落库 |
| POST | `/api/queues` | 创建队列（选择器或 `product_ids` 二选一 + 间隔秒数），去重后落库并启动 worker |
| POST | `/api/queues/{id}/entries` | **向运行中/暂停中的队列追加条目**（选择器或 `product_ids` + 同样的去重规则），不落 preview，直接返回追加/跳过名单 |
| GET | `/api/queues` | 队列列表（含进度计数） |
| GET | `/api/queues/{id}` | 队列详情（状态 + pending/running/done/failed/skipped 计数） |
| POST | `/api/queues/{id}/pause` | 暂停 |
| POST | `/api/queues/{id}/resume` | 恢复 |
| POST | `/api/queues/{id}/cancel` | 取消 |
| POST | `/api/queues/pause-all` | **全部暂停**：running/waiting 队列全部置 paused |
| POST | `/api/queues/resume-all` | **全部恢复**：所有 paused 队列置 waiting |

## 6. Worker 行为

- **全局唯一 worker**（不是一个队列一个）：循环找「当前 running 的队列」，逐条串行处理其条目；没有 running 队列时把创建最早的 `waiting` 队列提升为 `running` 再处理；连 waiting 都没有就空转等待。
- 每条流程：取出下一条 pending → 标记 running → 按 id 查商品（不存在 → `skipped`）→ 调用 `XianYuGateway.search(商品名, 1)`（本期为 Mock）→ 成功则计算中位数/均价/数量，回填商品统计字段（`Product::record_crawl_result`，回收价本期用均价占位）→ 条目 `done`；失败 → `failed` 并记录错误，继续下一条。
- 每条之间 sleep `interval_secs + 0..interval_secs 随机抖动`（取当前 running 队列的间隔配置）。
- 「全部暂停/全部恢复」不需要 worker 特殊处理：全部暂停后没有 running/waiting 队列，worker 自然空转；全部恢复后 waiting 队列回来了，worker 下轮自动顶起一个继续。
- 每轮循环检查当前队列状态：paused/cancelled → 释放执行位（下个循环自然提升 waiting 队列）；无 pending 条目 → 队列置 `done` 并释放执行位。
- **暂停是优雅暂停（跑完当前这条再停）**：不中途取消正在执行的条目——一次抓取就是一次 HTTP 请求，几秒完成，取消收益为零还会引入「被中断」状态。worker 只在条目边界检查队列状态，正在跑的条目自然落到 done/failed。
- **全局永远最多一个请求在飞**：单 worker 意味着 B 队列被提升后，它的第一条必然在 A 队列最后那条 in-flight 条目完成之后才开始（worker 正忙着处理 A 的条目，只有处理完才会进入下一轮发现 A 已暂停并提升 B）。提升后 B 的第一条立即开始，不额外等间隔。**提升只发生在 worker 循环边界**：从点下暂停到当前条目跑完之间，下一个队列保持 waiting，不存在「即将开始」的中间态。
- **间隔 sleep 切成 1 秒小片段**，每片醒来看一次队列状态：暂停/取消最迟 1 秒内生效，避免 worker 睡在长间隔里导致操作无响应。
- **支持运行中追加条目**：worker 每轮都重新查下一条 pending，新追加的条目会被自然拾取，无需重启 worker。追加只允许队列处于 waiting/running/paused；存在一个小竞争——worker 刚因条目耗尽置 `done` 时追加会被拒绝，此时应新建队列（报错信息会说明）。
- 每条处理打 `tracing::info!` 日志（本期测试用）。
- **并发与乱按安全性**：① 只有 worker 能执行条目和做提升决策，且每轮重新读库取最新状态，API 操作只改 status 字段；② 状态流转由实体方法把守，非法操作（对已暂停队列再暂停、乱点恢复等）只返回错误，不产生副作用；③ 全部暂停/恢复是幂等的批量更新。已知良性竞态：暂停/取消落在「状态检查与取下一条之间」的毫秒窗口时会多执行一条才生效——单次请求量级，对反爬无害，不做额外防护。
- **重启恢复**：服务启动时启动全局 worker；数据库中 running 的队列最多保留一个（最早创建的）为 running，其余降为 waiting；running 状态的条目重置为 pending。

## 7. 前端交互设计

页面新增「抓取队列」面板（置于商品管理上方），分两个区：**入队区**（选择器表单）和**队列列表**。商品管理面板加勾选框。核心交互流：

### 7.1 新建队列（按规则）

1. 在入队区勾选条件：三组标签 checkbox（全部包含 / 任一包含 / 排除）+ `stale_days` 天数 + 间隔秒数。
2. 点「预览」→ 下方显示「匹配 N 个，将新增 K 个，已在队列跳过 M 个」+ 两份名单。
3. 确认后点「加入队列」→ 创建成功，队列出现在列表顶部，自动开始轮询刷新。
- 「加入队列」允许跳过预览直接点，但预览结果会显著提示，推荐先预览。

### 7.2 新建队列（手动选择）

- 商品管理表格每行加勾选框，底部「选中加入队列」按钮 → confirm 展示将加入名单（及已在队列被跳过的）→ 确认后创建队列。
- 每行「抓取」快捷按钮 → 直接创建只含该商品的队列（单商品更新，无需任何规则）。

### 7.3 运行中追加条目

1. 队列列表每行有「追加」按钮（waiting/running/paused 可用，done/cancelled 禁用并置灰）。
2. 点击后入队区切换为**追加模式**：顶部显示黄色提示条「正在向队列 #N 追加条目」+「退出追加」按钮，「加入队列」按钮文案变为「追加到队列 #N」。
3. 用户按 7.1 选规则（或在商品管理勾选后按钮变为「追加到队列 #N」），提交到 `/api/queues/{id}/entries`。
4. 响应返回「实际追加 K 个，跳过 M 个」，toast 提示，队列进度总数即时增加，追加模式自动退出。

### 7.4 暂停 / 恢复 / 取消

- **顶部批量按钮**：「全部暂停」（存在 running/waiting 队列时可用）、「全部恢复」（存在 paused 队列时可用）两个按钮。点击后 toast 提示「已暂停 N 个队列」/「已恢复 N 个队列」。
- **行内按钮**照常随状态切换（任何时期都不置灰）：running 显示「暂停」「取消」；paused 显示「恢复」「取消」；waiting 显示「取消」；done/cancelled 无操作。
- 单队列暂停：释放执行位，下一个 waiting 队列自动顶上，badge 变灰「已暂停」。
- 单队列恢复：执行位空闲则直接续跑；已有队列在跑则变为「排队中」（waiting）。
- 取消：confirm 提示「剩余 N 条将不再执行（记录保留）」，确认后终止；取消 running 队列同样触发下一个队列顶上。

### 7.5 自动刷新

- 只要存在 waiting/running/paused 队列，每 2 秒轮询 `GET /api/queues` 刷新进度；全部终态后停止轮询并最后刷新一次商品列表（让回填的统计字段可见）。
- 页面加载时若发现活跃队列（重启恢复场景），自动开始轮询。

### 7.6 状态与提示文案

- 队列状态 badge：running 绿 / waiting 黄（「排队中」）/ paused 灰 / done 蓝 / cancelled 红。
- 进度列显示 `done+failed+skipped / total`，悬停显示各状态明细。
- 创建队列时若已有队列在跑，toast 提示「已加入排队，将在当前队列结束后自动开始」。
- 条目级失败原因本期不进 UI，只在接口里可查（`GET /api/queues/{id}` 返回计数；条目明细接口留待下期）。

## 8. 分层落位（按 AGENTS.md 约定）

- `domain/crawl_queue.rs`：Selector（validate/matches）、CrawlQueue、CrawlEntry、状态枚举与流转规则。（此文件已起草）
- `domain/repository.rs`：新增 `QueueRepository` 端口。
- `application/queue_service.rs`：预览/入队/暂停/恢复/取消/进度查询 + worker。
- `infrastructure/persistence/sqlite.rs`：两张新表 + `SqliteQueueRepository`。
- `interfaces/queue_handler.rs` + DTO + 路由。
- 前端新增「抓取队列」面板：选择器表单（三组标签勾选 + 天数 + 间隔）、预览结果、队列列表（进度 + 暂停/恢复/取消按钮），运行中队列自动轮询刷新。

## 9. 明确不做（本期）

- 真实闲鱼抓取（`HttpXianYuGateway` 仍留空，`GATEWAY=mock` 跑全链路）。
- 定时自动入队（如每天自动把七天未爬的入队）——后续可加 cron。
- 并发抓取、多 worker。
- 旧的 `POST /api/crawl`（一次性关键词抓取）暂时保留不动，队列稳定后再移除。

## 10. 验证计划

1. 建 2 个标签、3~4 个商品（不同标签组合），预览各选择器规则结果正确（a / a且b / a或b非c / stale_days / 省略 stale_days）。
2. 手动模式：`product_ids` 单个/多个入队、与选择器模式互斥校验、不存在商品报错。
3. 重复入队验证去重（已在队列的商品被跳过）。
3. mock 跑完队列：日志有逐条打印，商品统计字段（中位数/均价/数量/最后爬取时间）被回填。
4. 全局串行：建两个队列，第二个为 waiting 不执行；第一个暂停/跑完/取消后第二个自动顶上；恢复已暂停队列时若有队列在跑则转为 waiting。
5. 全部暂停/恢复：running + waiting 各一个队列时按「全部暂停」→ 都变 paused、无队列推进；再按「全部恢复」→ 全部转 waiting 并自动顶起一个（手动暂停的也一并恢复，为已知取舍）。
6. 中途暂停 → 不再推进 → 恢复 → 继续；取消 → 停止。
7. 入队后删除某商品 → 对应条目 skipped，其余正常。
8. 重启服务 → 未完成队列自动恢复执行（最多一个 running）。
