//! AI 自动打标签用例编排：同步/异步/取消 + 两个 AiTool 实现。

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio::sync::watch;

use crate::application::ports::{AiGateway, AiTool};
use crate::domain::ai_classify_task::{AiClassifyTask, ClassifyTaskStatus, ClassifyWarning};
use crate::domain::ai_tool_call::source as ai_source;
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::repository::{AiClassifyTaskRepository, ProductRepository, TagRepository};
use crate::domain::tag::Tag;

/// 同步路径单次上限
pub const SYNC_LIMIT: usize = 50;
/// 异步任务每批商品数
pub const CLASSIFY_BATCH_SIZE: usize = 50;
/// agent 最大轮数
const MAX_ROUNDS: u32 = 8;

// ---------- AI 工具 ----------

#[derive(Clone)]
pub struct ListTagsTool {
    tags: Arc<dyn TagRepository>,
}

impl ListTagsTool {
    pub fn new(tags: Arc<dyn TagRepository>) -> Self {
        Self { tags }
    }
}

#[async_trait]
impl AiTool for ListTagsTool {
    fn name(&self) -> &str {
        "list_tags"
    }

    fn description(&self) -> &str {
        "查询全部已启用的商品标签（id、名称、备注）。每次调用实时查询，返回当前库中最新的标签列表。"
    }

    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _args: JsonValue) -> Result<JsonValue, DomainError> {
        let tags = self.tags.list().await?;
        let enabled: Vec<&Tag> = tags.iter().filter(|t| t.enabled).collect();
        let list: Vec<JsonValue> = enabled
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "name": t.name.as_str(),
                    "remark": t.remark,
                })
            })
            .collect();
        Ok(serde_json::json!({
            "count": list.len(),
            "tags": list
        }))
    }
}

#[derive(Clone)]
pub struct ApplyProductTagsTool {
    products: Arc<dyn ProductRepository>,
    tags: Arc<dyn TagRepository>,
    allowed_ids: HashSet<i64>,
    /// 工具是否被模型调用过（用于检测「模型只总结不写库」）
    called: Arc<AtomicBool>,
}

impl ApplyProductTagsTool {
    pub fn new(
        products: Arc<dyn ProductRepository>,
        tags: Arc<dyn TagRepository>,
        allowed_ids: Vec<i64>,
        called: Arc<AtomicBool>,
    ) -> Self {
        Self {
            products,
            tags,
            allowed_ids: allowed_ids.into_iter().collect(),
            called,
        }
    }
}

#[async_trait]
impl AiTool for ApplyProductTagsTool {
    fn name(&self) -> &str {
        "apply_product_tags"
    }

    fn description(&self) -> &str {
        "为多个商品批量设置标签。接收 assignments 数组，每项包含 product_id、tag_ids（标签 id 列表，不允许编造，只能从 list_tags 返回的 id 中选择）、reason（简要说明分类理由）。不相关的商品可以传空 tag_ids。返回 applied（成功数）与 warnings（失败的条目及原因）。"
    }

    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "assignments": {
                    "type": "array",
                    "description": "商品标签分配列表",
                    "items": {
                        "type": "object",
                        "properties": {
                            "product_id": { "type": "integer", "description": "商品 id" },
                            "tag_ids": { "type": "array", "items": { "type": "integer" }, "description": "标签 id 列表" },
                            "reason": { "type": "string", "description": "简要分类理由" }
                        },
                        "required": ["product_id", "tag_ids"]
                    }
                }
            },
            "required": ["assignments"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        self.called.store(true, Ordering::Relaxed);

        let assignments = args
            .get("assignments")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut applied = 0u32;
        let mut warnings: Vec<JsonValue> = Vec::new();

        for item in &assignments {
            let product_id = match item.get("product_id").and_then(|v| v.as_i64()) {
                Some(id) => id,
                None => {
                    warnings.push(serde_json::json!({
                        "product_id": null,
                        "message": "缺少 product_id"
                    }));
                    continue;
                }
            };

            if !self.allowed_ids.contains(&product_id) {
                warnings.push(serde_json::json!({
                    "product_id": product_id,
                    "message": format!("商品 {product_id} 不在本次任务范围内，已跳过")
                }));
                continue;
            }

            let tag_ids: Vec<i64> = item
                .get("tag_ids")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
                .unwrap_or_default();

            let mut valid_tags = Vec::new();
            for tid in &tag_ids {
                match self.tags.find(*tid).await {
                    Ok(Some(t)) if t.enabled => valid_tags.push(*tid),
                    Ok(_) => {
                        warnings.push(serde_json::json!({
                            "product_id": product_id,
                            "message": format!("标签 {tid} 不存在或已禁用，已剔除")
                        }));
                    }
                    Err(e) => {
                        warnings.push(serde_json::json!({
                            "product_id": product_id,
                            "message": format!("查询标签 {tid} 失败: {e}")
                        }));
                    }
                }
            }

            match self.products.find(product_id).await {
                Ok(Some(mut product)) => {
                    product.tag_ids = valid_tags;
                    if let Err(e) = self.products.update(&product).await {
                        warnings.push(serde_json::json!({
                            "product_id": product_id,
                            "message": format!("写入标签失败: {e}")
                        }));
                    } else {
                        applied += 1;
                    }
                }
                Ok(None) => {
                    warnings.push(serde_json::json!({
                        "product_id": product_id,
                        "message": format!("商品 {product_id} 不存在（可能已被删除），已跳过")
                    }));
                }
                Err(e) => {
                    warnings.push(serde_json::json!({
                        "product_id": product_id,
                        "message": format!("查询商品失败: {e}")
                    }));
                }
            }
        }

        let result = serde_json::json!({
            "applied": applied,
            "warnings": warnings,
            "total_assignments": assignments.len()
        });
        Ok(result)
    }
}

// ---------- 服务 ----------

pub struct ClassifyService {
    tasks: Arc<dyn AiClassifyTaskRepository>,
    products: Arc<dyn ProductRepository>,
    tags: Arc<dyn TagRepository>,
    ai_gateway: Arc<dyn AiGateway>,
}

impl ClassifyService {
    pub fn new(
        tasks: Arc<dyn AiClassifyTaskRepository>,
        products: Arc<dyn ProductRepository>,
        tags: Arc<dyn TagRepository>,
        ai_gateway: Arc<dyn AiGateway>,
    ) -> Self {
        Self {
            tasks,
            products,
            tags,
            ai_gateway,
        }
    }

    /// 同步路径：≤ 50 商品，一次 run_agent 完成，直接返回结果
    pub async fn classify_sync(
        &self,
        product_ids: Vec<i64>,
    ) -> Result<ClassifySyncResult, DomainError> {
        if product_ids.len() > SYNC_LIMIT {
            return Err(DomainError::InvalidInput(format!(
                "同步路径最多 {SYNC_LIMIT} 个商品，当前 {} 个，请走异步任务", product_ids.len()
            )));
        }

        self.validate_product_ids(&product_ids).await?;
        self.check_tags_available().await?;

        let product_infos = self.load_product_infos(&product_ids).await?;
        let write_called = Arc::new(AtomicBool::new(false));
        let tools: Vec<Arc<dyn AiTool>> = vec![
            Arc::new(ListTagsTool::new(self.tags.clone())),
            Arc::new(ApplyProductTagsTool::new(
                self.products.clone(),
                self.tags.clone(),
                product_ids.clone(),
                write_called.clone(),
            )),
        ];

        let system = build_system_prompt();
        let user = build_user_prompt(&product_infos);
        let output = self
            .ai_gateway
            .run_agent(&system, &user, &tools, MAX_ROUNDS, ai_source::CLASSIFY, None)
            .await?;

        // 模型光总结不调写工具 = 什么都没写入，按失败处理（设计文档 3.2）
        if !write_called.load(Ordering::Relaxed) {
            return Err(DomainError::InvalidState(
                "AI 未执行打标签操作，请重试".into(),
            ));
        }

        let (suggestions, warnings) = self.collect_results(&product_infos).await;
        Ok(ClassifySyncResult {
            summary: output,
            suggestions,
            warnings,
        })
    }

    /// 创建异步分类任务并立即返回任务信息（后台 tokio task 执行）
    pub async fn create_classify_task(
        &self,
        product_ids: Vec<i64>,
    ) -> Result<AiClassifyTask, DomainError> {
        self.validate_product_ids(&product_ids).await?;
        self.check_tags_available().await?;

        let mut task = AiClassifyTask::new(product_ids, CLASSIFY_BATCH_SIZE);
        task.start()?;
        self.tasks.save(&task).await?;

        let service = ClassifyService {
            tasks: self.tasks.clone(),
            products: self.products.clone(),
            tags: self.tags.clone(),
            ai_gateway: self.ai_gateway.clone(),
        };
        let task_id = task.id.clone();
        let (cancel_tx, cancel_rx) = watch::channel(false);

        // 存储 cancel 通道供 cancel 接口使用
        let cancel_holder = super::super::cancel_token::register(task_id.clone(), cancel_tx);

        tokio::spawn(async move {
            service.run_async_task(task_id, cancel_rx).await;
            drop(cancel_holder);
        });

        Ok(task)
    }

    /// 取消运行中的任务
    pub async fn cancel_classify_task(&self, id: &str) -> Result<AiClassifyTask, DomainError> {
        let mut task = self
            .tasks
            .find(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("分类任务 {id}")))?;

        task.cancel()?;
        self.tasks.save(&task).await?;

        super::super::cancel_token::send(id);

        Ok(task)
    }

    /// 查询任务状态
    pub async fn get_classify_task(&self, id: &str) -> Result<AiClassifyTask, DomainError> {
        self.tasks
            .find(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("分类任务 {id}")))
    }

    // ---------- 私有方法 ----------

    async fn validate_product_ids(&self, product_ids: &[i64]) -> Result<(), DomainError> {
        for id in product_ids {
            if self.products.find(*id).await?.is_none() {
                return Err(DomainError::NotFound(format!("商品 {id}")));
            }
        }
        Ok(())
    }

    async fn check_tags_available(&self) -> Result<(), DomainError> {
        let tags = self.tags.list().await?;
        let enabled: Vec<&Tag> = tags.iter().filter(|t| t.enabled).collect();
        if enabled.is_empty() {
            return Err(DomainError::InvalidState(
                "暂无可用标签，请先创建并启用标签".into(),
            ));
        }
        Ok(())
    }

    async fn load_product_infos(&self, product_ids: &[i64]) -> Result<Vec<ProductInfo>, DomainError> {
        let mut infos = Vec::with_capacity(product_ids.len());
        for id in product_ids {
            if let Some(p) = self.products.find(*id).await? {
                infos.push(ProductInfo {
                    id: p.id,
                    name: p.name.as_str().to_string(),
                    remark: p.remark.clone(),
                });
            }
        }
        Ok(infos)
    }

    /// 收集工具已写入的结果（重新查询商品标签）
    async fn collect_results(
        &self,
        product_infos: &[ProductInfo],
    ) -> (Vec<ClassifySuggestion>, Vec<String>) {
        let mut suggestions = Vec::new();
        let mut warnings = Vec::new();
        for info in product_infos {
            if let Ok(Some(p)) = self.products.find(info.id).await {
                suggestions.push(ClassifySuggestion {
                    product_id: p.id,
                    tag_ids: p.tag_ids.clone(),
                });
            } else {
                warnings.push(format!("商品 {} 查询失败", info.id));
            }
        }
        (suggestions, warnings)
    }

    /// 后台执行异步任务：按批次串行调用 AI
    async fn run_async_task(&self, task_id: String, mut cancel_rx: watch::Receiver<bool>) {
        let mut task = match self.tasks.find(&task_id).await {
            Ok(Some(t)) => t,
            _ => return,
        };

        loop {
            let batch = match task.next_batch() {
                Some(b) => b,
                None => {
                    let _ = task.finish();
                    let _ = self.tasks.save(&task).await;
                    return;
                }
            };

            let batch_succeeded;
            let batch_failed;
            let batch_warnings;

            // 构造本批工具（含产品白名单）
            let batch_ids = batch.clone();
            let write_called = Arc::new(AtomicBool::new(false));
            let tools: Vec<Arc<dyn AiTool>> = vec![
                Arc::new(ListTagsTool::new(self.tags.clone())),
                Arc::new(ApplyProductTagsTool::new(
                    self.products.clone(),
                    self.tags.clone(),
                    batch_ids.clone(),
                    write_called.clone(),
                )),
            ];

            let infos = match self.load_product_infos(&batch).await {
                Ok(i) => i,
                Err(e) => {
                    tracing::warn!("分类任务 {task_id} 加载商品信息失败: {e}");
                    batch_failed = batch.len();
                    batch_succeeded = 0;
                    batch_warnings = vec![ClassifyWarning {
                        product_id: 0,
                        message: format!("加载商品信息失败: {e}"),
                    }];
                    task.record_batch(batch_succeeded, batch_failed, batch_warnings);
                    let _ = self.tasks.save(&task).await;
                    continue;
                }
            };

            let system = build_system_prompt();
            let user = build_user_prompt(&infos);

        let output = tokio::select! {
            _ = cancel_rx.changed() => {
                // 取消信号：直接标记 cancelled 退出
                if let Ok(Some(mut t)) = self.tasks.find(&task_id).await {
                    t.status = ClassifyTaskStatus::Cancelled;
                    t.finished_at = Some(now_unix());
                    let _ = self.tasks.save(&t).await;
                }
                return;
            }
            result = self.ai_gateway.run_agent(&system, &user, &tools, MAX_ROUNDS, ai_source::CLASSIFY, None) => {
                result
            }
        };

            match output {
                Ok(_summary) => {
                    if !write_called.load(Ordering::Relaxed) {
                        // 模型光总结不调写工具 = 本批什么都没写入
                        batch_succeeded = 0;
                        batch_failed = infos.len();
                        batch_warnings = vec![ClassifyWarning {
                            product_id: 0,
                            message: "AI 未执行打标签操作".into(),
                        }];
                    } else {
                        // 收集实际写入结果
                        let mut warnings_list = Vec::new();
                        let mut ok = 0usize;
                        let mut fail = 0usize;
                        for info in &infos {
                            match self.products.find(info.id).await {
                                Ok(Some(_)) => {
                                    ok += 1;
                                }
                                Ok(None) => {
                                    fail += 1;
                                    warnings_list.push(ClassifyWarning {
                                        product_id: info.id,
                                        message: "商品被删除".into(),
                                    });
                                }
                                Err(e) => {
                                    fail += 1;
                                    warnings_list.push(ClassifyWarning {
                                        product_id: info.id,
                                        message: format!("查询失败: {e}"),
                                    });
                                }
                            }
                        }
                        batch_succeeded = ok;
                        batch_failed = fail;
                        batch_warnings = warnings_list;
                    }
                }
                Err(e) => {
                    tracing::warn!("分类任务 {task_id} batch 失败: {e}");
                    batch_succeeded = 0;
                    batch_failed = infos.len();
                    batch_warnings = vec![ClassifyWarning {
                        product_id: 0,
                        message: format!("AI 请求失败: {e}"),
                    }];
                }
            }

            task.record_batch(batch_succeeded, batch_failed, batch_warnings);
            let _ = self.tasks.save(&task).await;

            // 检查是否被取消
            if *cancel_rx.borrow() {
                if let Ok(Some(mut t)) = self.tasks.find(&task_id).await {
                    t.cancel().ok();
                    let _ = self.tasks.save(&t).await;
                }
                return;
            }
        }
    }
}

// ---------- 辅助函数 ----------

struct ProductInfo {
    id: i64,
    name: String,
    remark: Option<String>,
}

fn build_system_prompt() -> String {
    r#"你是一名商品分类助手。你的任务是为给定商品选择合适的标签。

硬性规则：
1. 你只能从 list_tags 工具返回的标签 id 中选择，严禁编造或猜测标签 id。
2. 必须通过 apply_product_tags 工具提交结果，不要直接在文本中回答标签。
3. 不相关的商品可以传空 tag_ids（[]），表示不挂任何标签。
4. 少量多次原则：如果商品数量较多，可每批提交一部分并说明依据。
5. reason 字段简要说明分类理由（如"该商品属于相机类"）。
6. 你的最终回复只作总结，不要包含 JSON 数据（所有数据通过工具提交）。
7. 如果 list_tags 返回的标签中没有适合该商品的，不要强行匹配，传空 tag_ids 即可。"#
        .to_string()
}

fn build_user_prompt(products: &[ProductInfo]) -> String {
    let items: Vec<JsonValue> = products
        .iter()
        .map(|p| {
            let mut obj = serde_json::json!({
                "id": p.id,
                "name": p.name,
            });
            if let Some(ref r) = p.remark {
                obj["remark"] = JsonValue::String(r.clone());
            }
            obj
        })
        .collect();
    format!(
        "请为以下 {} 个商品分配合适的标签：\n{}",
        products.len(),
        serde_json::to_string_pretty(&items).unwrap_or_default()
    )
}

// ---------- 公开类型 ----------

#[derive(Debug)]
pub struct ClassifySyncResult {
    pub summary: String,
    pub suggestions: Vec<ClassifySuggestion>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClassifySuggestion {
    pub product_id: i64,
    pub tag_ids: Vec<i64>,
}
