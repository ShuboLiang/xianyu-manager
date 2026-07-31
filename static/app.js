const statusEl = document.getElementById('status');
const tableEl = document.getElementById('itemTable');
const tagTableEl = document.getElementById('tagTable');
const productTableEl = document.getElementById('productTable');
const queueTableEl = document.getElementById('queueTable');
const historyTableEl = document.getElementById('historyTable');
let editingTagId = null;
let editingProductId = null;
let cachedTags = [];
let appendTargetQueueId = null;   // 追加模式：目标队列 id
let queueWasActive = false;        // 上轮轮询是否有活跃队列

async function checkHealth() {
    try {
        const res = await fetch('/api/health');
        const body = await res.json();
        if (body.code === 0) {
            statusEl.textContent = '服务正常';
            statusEl.classList.add('ok');
        } else {
            throw new Error(body.message);
        }
    } catch {
        statusEl.textContent = '服务不可用';
        statusEl.classList.add('down');
    }
}

function renderItems(items) {
    if (!items || items.length === 0) {
        tableEl.innerHTML = '<tr><td colspan="5" class="empty">暂无数据</td></tr>';
        return;
    }
    tableEl.innerHTML = items.map(it => `
        <tr>
            <td>${it.title}</td>
            <td>¥${it.price}</td>
            <td>${it.seller}</td>
            <td>${it.crawled_at}</td>
            <td><a href="${it.url}" target="_blank">查看</a></td>
        </tr>
    `).join('');
}

async function loadItems() {
    const res = await fetch('/api/items');
    const body = await res.json();
    renderItems(body.data);
}

document.getElementById('refreshBtn').addEventListener('click', loadItems);

// ---------- 标签管理 ----------

function renderTags(tags) {
    if (!tags || tags.length === 0) {
        tagTableEl.innerHTML = '<tr><td colspan="4" class="empty">暂无标签</td></tr>';
        return;
    }
    tagTableEl.innerHTML = tags.map(t => `
        <tr>
            <td>${t.name}</td>
            <td>${t.enabled ? '启用' : '停用'}</td>
            <td>${t.remark || '-'}</td>
            <td>
                <a href="#" onclick="editTag(${t.id}); return false;">编辑</a>
                <a href="#" onclick="toggleTag(${t.id}, ${t.enabled}); return false;">${t.enabled ? '停用' : '启用'}</a>
                <a href="#" onclick="deleteTag(${t.id}); return false;">删除</a>
            </td>
        </tr>
    `).join('');
}

async function loadTags() {
    const res = await fetch('/api/tags');
    const body = await res.json();
    cachedTags = body.data || [];
    renderTags(cachedTags);
    renderTagOptions();
    renderSelectorGroups();
}

// 队列选择器的三组标签勾选（AND / OR / NOT）
function renderSelectorGroups() {
    for (const id of ['selAll', 'selAny', 'selExclude']) {
        const box = document.getElementById(id);
        const checked = new Set([...box.querySelectorAll('input:checked')].map(i => i.value));
        if (cachedTags.length === 0) {
            box.innerHTML = '<span class="muted">暂无标签</span>';
            continue;
        }
        box.innerHTML = cachedTags.map(t => `
            <label class="tag-check">
                <input type="checkbox" value="${t.id}" ${checked.has(String(t.id)) ? 'checked' : ''}>
                ${t.name}
            </label>
        `).join('');
    }
}

function renderTagOptions() {
    const box = document.getElementById('productTagList');
    const checked = new Set(
        [...box.querySelectorAll('input:checked')].map(i => i.value)
    );
    if (cachedTags.length === 0) {
        box.innerHTML = '<span class="muted">暂无标签可勾选</span>';
        return;
    }
    box.innerHTML = cachedTags.map(t => `
        <label class="tag-check">
            <input type="checkbox" value="${t.id}" ${checked.has(String(t.id)) ? 'checked' : ''}>
            ${t.name}
        </label>
    `).join('');
}

function selectedTagIds() {
    return [...document.querySelectorAll('#productTagList input:checked')]
        .map(i => Number(i.value));
}

function setSelectedTagIds(ids) {
    document.querySelectorAll('#productTagList input').forEach(i => {
        i.checked = ids.includes(Number(i.value));
    });
}

async function submitTag() {
    const name = document.getElementById('tagName').value.trim();
    const remark = document.getElementById('tagRemark').value.trim();
    if (!name) {
        alert('标签名不能为空');
        return;
    }
    const isEdit = editingTagId !== null;
    const res = await fetch(isEdit ? `/api/tags/${editingTagId}` : '/api/tags', {
        method: isEdit ? 'PUT' : 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, remark: remark || null }),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert((isEdit ? '更新' : '创建') + '失败: ' + body.message);
        return;
    }
    resetTagForm();
    loadTags().then(loadProducts);
}

async function editTag(id) {
    const res = await fetch(`/api/tags/${id}`);
    const body = await res.json();
    if (body.code !== 0) {
        alert('加载标签失败: ' + body.message);
        return;
    }
    const t = body.data;
    editingTagId = t.id;
    document.getElementById('tagName').value = t.name;
    document.getElementById('tagRemark').value = t.remark || '';
    document.getElementById('tagSubmitBtn').textContent = '保存修改';
    document.getElementById('tagCancelBtn').hidden = false;
}

async function toggleTag(id, enabled) {
    const res = await fetch(`/api/tags/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: !enabled }),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert('切换状态失败: ' + body.message);
    }
    loadTags().then(loadProducts);
}

async function deleteTag(id) {
    // 删除前提示该标签正被哪些商品使用（删除后这些商品将移除此标签）
    const usageRes = await fetch(`/api/tags/${id}/products`);
    const usageBody = await usageRes.json();
    if (usageBody.code !== 0) {
        alert('查询标签使用情况失败: ' + usageBody.message);
        return;
    }
    const used = usageBody.data || [];
    const hint = used.length === 0
        ? '确定删除该标签？'
        : `该标签正被 ${used.length} 个商品使用：\n${used.map(p => '· ' + p.name).join('\n')}\n\n删除后这些商品将移除此标签。确定删除？`;
    if (!confirm(hint)) return;
    const res = await fetch(`/api/tags/${id}`, { method: 'DELETE' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('删除失败: ' + body.message);
    }
    loadTags().then(loadProducts);
}

function resetTagForm() {
    editingTagId = null;
    document.getElementById('tagName').value = '';
    document.getElementById('tagRemark').value = '';
    document.getElementById('tagSubmitBtn').textContent = '添加标签';
    document.getElementById('tagCancelBtn').hidden = true;
}

document.getElementById('tagSubmitBtn').addEventListener('click', submitTag);
document.getElementById('tagCancelBtn').addEventListener('click', resetTagForm);

// ---------- 待爬取商品管理 ----------

function fmtPrice(v) {
    return v === null || v === undefined ? '-' : '¥' + v.toFixed(2);
}

function fmtTime(unix) {
    if (!unix) return '-';
    return new Date(unix * 1000).toLocaleString('zh-CN', { hour12: false });
}

function renderProducts(products) {
    if (!products || products.length === 0) {
        productTableEl.innerHTML = '<tr><td colspan="10" class="empty">暂无商品</td></tr>';
        return;
    }
    productTableEl.innerHTML = products.map(p => `
        <tr>
            <td><input type="checkbox" class="prod-check" value="${p.id}"></td>
            <td>${p.name}</td>
            <td>${p.tag_names.length ? p.tag_names.join('、') : '<span class="muted">无标签</span>'}</td>
            <td>${fmtPrice(p.median_price)}</td>
            <td>${fmtPrice(p.avg_price)}</td>
            <td>${p.crawled_count ?? '-'}</td>
            <td>${fmtTime(p.last_crawled_at)}</td>
            <td>${fmtPrice(p.recycle_price)}</td>
            <td>${p.remark || '-'}</td>
            <td>
                <a href="#" onclick="crawlProduct(${p.id}); return false;">抓取</a>
                <a href="#" onclick="editProduct(${p.id}); return false;">编辑</a>
                <a href="#" onclick="deleteProduct(${p.id}); return false;">删除</a>
            </td>
        </tr>
    `).join('');
}

async function loadProducts() {
    const res = await fetch('/api/products');
    const body = await res.json();
    renderProducts(body.data);
}

async function submitProduct() {
    const name = document.getElementById('productName').value.trim();
    const remark = document.getElementById('productRemark').value.trim();
    if (!name) {
        alert('商品名不能为空');
        return;
    }
    const isEdit = editingProductId !== null;
    const payload = { name, tag_ids: selectedTagIds(), remark: remark || null };
    const res = await fetch(isEdit ? `/api/products/${editingProductId}` : '/api/products', {
        method: isEdit ? 'PUT' : 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert((isEdit ? '更新' : '创建') + '失败: ' + body.message);
        return;
    }
    resetProductForm();
    loadProducts();
}

async function editProduct(id) {
    const res = await fetch(`/api/products/${id}`);
    const body = await res.json();
    if (body.code !== 0) {
        alert('加载商品失败: ' + body.message);
        return;
    }
    const p = body.data;
    editingProductId = p.id;
    document.getElementById('productName').value = p.name;
    setSelectedTagIds(p.tag_ids);
    document.getElementById('productRemark').value = p.remark || '';
    document.getElementById('productSubmitBtn').textContent = '保存修改';
    document.getElementById('productCancelBtn').hidden = false;
}

async function deleteProduct(id) {
    if (!confirm('确定删除该商品？')) return;
    const res = await fetch(`/api/products/${id}`, { method: 'DELETE' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('删除失败: ' + body.message);
    }
    loadProducts();
}

function resetProductForm() {
    editingProductId = null;
    document.getElementById('productName').value = '';
    setSelectedTagIds([]);
    document.getElementById('productRemark').value = '';
    document.getElementById('productSubmitBtn').textContent = '添加商品';
    document.getElementById('productCancelBtn').hidden = true;
}

document.getElementById('productSubmitBtn').addEventListener('click', submitProduct);
document.getElementById('productCancelBtn').addEventListener('click', resetProductForm);

// ---------- 批量导入 ----------

function renderBatchImportTags() {
    const box = document.getElementById('batchImportTagList');
    if (cachedTags.length === 0) {
        box.innerHTML = '<span class="muted">暂无标签</span>';
        return;
    }
    box.innerHTML = cachedTags.map(t => `
        <label class="tag-check">
            <input type="checkbox" value="${t.id}">
            ${t.name}
        </label>
    `).join('');
}

function batchImportTagIds() {
    return [...document.querySelectorAll('#batchImportTagList input:checked')]
        .map(i => Number(i.value));
}

function openBatchImport() {
    renderBatchImportTags();
    document.getElementById('batchImportTextarea').value = '';
    document.getElementById('batchImportResult').hidden = true;
    document.getElementById('batchImportModal').hidden = false;
}

function closeBatchImport() {
    document.getElementById('batchImportModal').hidden = true;
}

async function submitBatchImport() {
    const text = document.getElementById('batchImportTextarea').value;
    const names = text.split(/\n/).map(s => s.trim()).filter(s => s.length > 0);
    if (names.length === 0) {
        alert('请输入至少一个商品名');
        return;
    }
    if (names.length > 1000) {
        alert(`最多 1000 条，当前 ${names.length} 条`);
        return;
    }
    const tag_ids = batchImportTagIds();
    const btn = document.getElementById('batchImportSubmitBtn');
    btn.disabled = true;
    btn.textContent = '提交中...';

    try {
        const res = await fetch('/api/products/batch', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ names, tag_ids: tag_ids.length ? tag_ids : null }),
        });
        const body = await res.json();
        if (body.code !== 0) {
            alert('导入失败: ' + body.message);
            return;
        }
        const { created, skipped } = body.data;
        const resultDiv = document.getElementById('batchImportResult');
        resultDiv.hidden = false;
        let html = `创建 <b>${created.length}</b> 条`;
        if (created.length) {
            html += '：' + created.map(p => p.name).join('、');
        }
        if (skipped.length) {
            html += `<br>跳过 <b>${skipped.length}</b> 条：` +
                skipped.map(s => `${s.name}（${s.reason}）`).join('、');
        }
        resultDiv.innerHTML = html;
        if (created.length > 0) {
            document.getElementById('batchImportTextarea').value = '';
            loadProducts();
        }
    } finally {
        btn.disabled = false;
        btn.textContent = '提交导入';
    }
}

document.getElementById('batchImportBtn').addEventListener('click', openBatchImport);
document.getElementById('batchImportCloseBtn').addEventListener('click', closeBatchImport);
document.getElementById('batchImportSubmitBtn').addEventListener('click', submitBatchImport);

// ---------- AI 自动打标签 ----------

let classifyTaskId = null;
let classifyPollTimer = null;

function selectedProductIds() {
    return [...document.querySelectorAll('.prod-check:checked')]
        .map(i => Number(i.value));
}

async function aiClassify() {
    const ids = selectedProductIds();
    if (ids.length === 0) {
        alert('请先勾选商品');
        return;
    }

    if (ids.length <= 50) {
        const btn = document.getElementById('aiClassifyBtn');
        btn.disabled = true;
        btn.textContent = 'AI 分类中...';
        try {
            const res = await fetch('/api/ai/classify-products', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ product_ids: ids }),
            });
            const body = await res.json();
            if (body.code !== 0) {
                alert('AI 分类失败: ' + body.message);
                return;
            }
            const { suggestions, warnings } = body.data;
            let msg = `AI 已完成分类，涉及 ${suggestions.length} 个商品`;
            if (warnings.length) {
                msg += `，有 ${warnings.length} 条警告：\n` + warnings.join('\n');
            }
            alert(msg);
            loadProducts();
            loadAiCalls();
        } finally {
            btn.disabled = false;
            btn.textContent = 'AI 自动打标签';
        }
    } else {
        const res = await fetch('/api/ai/classify-tasks', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ product_ids: ids }),
        });
        const body = await res.json();
        if (body.code !== 0) {
            alert('创建分类任务失败: ' + body.message);
            return;
        }
        classifyTaskId = body.data.id;
        showClassifyProgress();
        pollClassifyTask();
    }
}

function showClassifyProgress() {
    document.getElementById('classifyProgress').hidden = false;
    updateClassifyProgress({ processed: 0, total: 0, succeeded: 0, failed: 0, status: 'running' });
}

function updateClassifyProgress(task) {
    const pct = task.total > 0 ? Math.round(task.processed / task.total * 100) : 0;
    document.getElementById('classifyProgressFill').style.width = pct + '%';
    let statusText = `已处理 ${task.processed}/${task.total}`;
    if (task.failed > 0) statusText += `，失败 ${task.failed}`;
    statusText += ` | 状态: ${task.status}`;
    if (task.error) statusText += ` | 错误: ${task.error}`;
    document.getElementById('classifyProgressText').textContent = statusText;
}

async function pollClassifyTask() {
    if (!classifyTaskId) return;
    const res = await fetch(`/api/ai/classify-tasks/${classifyTaskId}`);
    const body = await res.json();
    if (body.code !== 0) {
        updateClassifyProgress({ processed: 0, total: 0, succeeded: 0, failed: 0, status: 'failed', error: body.message });
        stopPolling();
        return;
    }
    const task = body.data;
    updateClassifyProgress(task);
    if (['done', 'failed', 'cancelled'].includes(task.status)) {
        stopPolling();
        document.getElementById('classifyCancelBtn').hidden = true;
        loadProducts();
        loadAiCalls();
        setTimeout(hideClassifyProgress, 3000);
        return;
    }
    classifyPollTimer = setTimeout(pollClassifyTask, 2000);
}

function stopPolling() {
    if (classifyPollTimer) {
        clearTimeout(classifyPollTimer);
        classifyPollTimer = null;
    }
}

async function cancelClassifyTask() {
    if (!classifyTaskId) return;
    const res = await fetch(`/api/ai/classify-tasks/${classifyTaskId}/cancel`, { method: 'POST' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('取消失败: ' + body.message);
        return;
    }
    updateClassifyProgress(body.data);
    stopPolling();
    loadProducts();
}

function hideClassifyProgress() {
    document.getElementById('classifyProgress').hidden = true;
    classifyTaskId = null;
    stopPolling();
}

document.getElementById('aiClassifyBtn').addEventListener('click', aiClassify);
document.getElementById('classifyCancelBtn').addEventListener('click', cancelClassifyTask);

// ---------- 抓取队列 ----------

function checkedIds(containerId) {
    return [...document.querySelectorAll(`#${containerId} input:checked`)]
        .map(i => Number(i.value));
}

function collectSelector() {
    const stale = document.getElementById('staleDays').value;
    return {
        tag_all: checkedIds('selAll'),
        tag_any: checkedIds('selAny'),
        tag_exclude: checkedIds('selExclude'),
        stale_days: stale ? Number(stale) : null,
    };
}

function selectorIsEmpty(sel) {
    return sel.tag_all.length === 0 && sel.tag_any.length === 0
        && sel.tag_exclude.length === 0 && sel.stale_days === null;
}

async function previewSelector() {
    const selector = collectSelector();
    if (selectorIsEmpty(selector)) {
        alert('请至少选择一个标签条件或填写天数');
        return;
    }
    const res = await fetch('/api/queues/preview', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ selector }),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert('预览失败: ' + body.message);
        return;
    }
    const { to_add, skipped } = body.data;
    const box = document.getElementById('previewResult');
    box.hidden = false;
    box.innerHTML = `将新增 <b>${to_add.length}</b> 个${to_add.length ? '：' + to_add.map(p => p.name).join('、') : ''}` +
        (skipped.length ? `<br>已在队列，跳过 <b>${skipped.length}</b> 个：${skipped.map(p => p.name).join('、')}` : '');
}

async function enqueueBySelector() {
    const selector = collectSelector();
    if (selectorIsEmpty(selector)) {
        alert('请至少选择一个标签条件或填写天数');
        return;
    }
    const interval = Number(document.getElementById('intervalSecs').value) || 3;
    const isAppend = appendTargetQueueId !== null;
    const url = isAppend ? `/api/queues/${appendTargetQueueId}/entries` : '/api/queues';
    const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ selector, interval_secs: interval }),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert((isAppend ? '追加' : '入队') + '失败: ' + body.message);
        return;
    }
    reportEnqueue(body.data, isAppend);
}

function reportEnqueue(data, isAppend) {
    let msg = `${isAppend ? '追加' : '入队'}成功：新增 ${data.added.length} 个`;
    if (data.skipped.length) msg += `，跳过 ${data.skipped.length} 个（已在队列）`;
    if (!isAppend && data.status === 'waiting') msg += '\n已有队列在执行，本队列进入排队，将自动开始';
    alert(msg);
    document.getElementById('previewResult').hidden = true;
    if (isAppend) exitAppendMode();
    loadQueues();
}

async function crawlSelected() {
    const ids = [...document.querySelectorAll('.prod-check:checked')].map(i => Number(i.value));
    if (ids.length === 0) {
        alert('请先勾选商品');
        return;
    }
    const isAppend = appendTargetQueueId !== null;
    const interval = Number(document.getElementById('intervalSecs').value) || 3;
    const url = isAppend ? `/api/queues/${appendTargetQueueId}/entries` : '/api/queues';
    if (!confirm(`将 ${ids.length} 个商品${isAppend ? `追加到队列 #${appendTargetQueueId}` : '加入新队列'}？`)) return;
    const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ product_ids: ids, interval_secs: interval }),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert('入队失败: ' + body.message);
        return;
    }
    document.querySelectorAll('.prod-check:checked').forEach(i => i.checked = false);
    reportEnqueue(body.data, isAppend);
}

async function crawlProduct(id) {
    const interval = Number(document.getElementById('intervalSecs').value) || 3;
    const isAppend = appendTargetQueueId !== null;
    const url = isAppend ? `/api/queues/${appendTargetQueueId}/entries` : '/api/queues';
    const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ product_ids: [id], interval_secs: interval }),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert('入队失败: ' + body.message);
        return;
    }
    reportEnqueue(body.data, isAppend);
}

const QUEUE_STATUS_TEXT = {
    waiting: '排队中', running: '执行中', paused: '已暂停', done: '已完成', cancelled: '已取消',
};

function queueActions(q) {
    const links = [];
    if (q.status === 'running') {
        links.push(`<a href="#" onclick="pauseQueue(${q.id}); return false;">暂停</a>`);
    }
    if (q.status === 'paused') {
        links.push(`<a href="#" onclick="resumeQueue(${q.id}); return false;">恢复</a>`);
    }
    if (['waiting', 'running', 'paused'].includes(q.status)) {
        links.push(`<a href="#" onclick="appendQueue(${q.id}); return false;">追加</a>`);
        links.push(`<a href="#" onclick="cancelQueue(${q.id}); return false;">取消</a>`);
    }
    return links.join(' ') || '<span class="muted">-</span>';
}

function queueRow(q, actions) {
    return `
        <tr>
            <td>#${q.id}</td>
            <td><span class="badge badge-${q.status}">${QUEUE_STATUS_TEXT[q.status] || q.status}</span></td>
            <td title="待 ${q.pending} / 成 ${q.done} / 败 ${q.failed} / 跳 ${q.skipped}">${q.done + q.failed + q.skipped}/${q.total}</td>
            <td>${q.interval_secs}s</td>
            <td>${fmtTime(q.created_at)}</td>
            <td>${actions}</td>
        </tr>
    `;
}

async function loadQueues() {
    const res = await fetch('/api/queues');
    const body = await res.json();
    const queues = body.data || [];
    const active = queues.filter(q => ['waiting', 'running', 'paused'].includes(q.status));
    const history = queues.filter(q => !['waiting', 'running', 'paused'].includes(q.status));

    if (active.length === 0) {
        queueTableEl.innerHTML = '<tr><td colspan="6" class="empty">暂无活跃队列</td></tr>';
    } else {
        queueTableEl.innerHTML = active.map(q => queueRow(q, queueActions(q))).join('');
    }

    const toggle = document.getElementById('historyToggle');
    document.getElementById('historyCount').textContent = history.length;
    toggle.hidden = history.length === 0;
    if (history.length === 0) {
        document.getElementById('historyTableWrap').hidden = true;
        document.getElementById('historyArrow').textContent = '▸';
    }
    historyTableEl.innerHTML = history.map(q =>
        queueRow(q, `<a href="#" onclick="deleteQueue(${q.id}); return false;">删除</a>`)
    ).join('');

    const running = active.some(q => ['waiting', 'running'].includes(q.status));
    if (running) {
        queueWasActive = true;
        setTimeout(loadQueues, 2000);
    } else if (queueWasActive) {
        // 刚全部结束：最后刷新一次商品统计和原始数据
        queueWasActive = false;
        loadProducts();
        loadItems();
    }
}

function toggleHistory() {
    const wrap = document.getElementById('historyTableWrap');
    wrap.hidden = !wrap.hidden;
    document.getElementById('historyArrow').textContent = wrap.hidden ? '▸' : '▾';
}

async function deleteQueue(id) {
    if (!confirm(`确定删除队列 #${id}？队列及其条目记录将被永久删除。`)) return;
    const res = await fetch(`/api/queues/${id}`, { method: 'DELETE' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('删除失败: ' + body.message);
    }
    loadQueues();
}

async function pauseQueue(id) { await queueOp(`/api/queues/${id}/pause`); }
async function resumeQueue(id) { await queueOp(`/api/queues/${id}/resume`); }

async function cancelQueue(id) {
    if (!confirm('确定取消该队列？剩余条目将不再执行（记录保留）。')) return;
    await queueOp(`/api/queues/${id}/cancel`);
}

async function queueOp(url) {
    const res = await fetch(url, { method: 'POST' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('操作失败: ' + body.message);
    }
    loadQueues();
}

function appendQueue(id) {
    appendTargetQueueId = id;
    document.getElementById('appendQueueId').textContent = id;
    document.getElementById('appendBanner').hidden = false;
    document.getElementById('enqueueBtn').textContent = `追加到队列 #${id}`;
    window.scrollTo({ top: 0, behavior: 'smooth' });
}

function exitAppendMode() {
    appendTargetQueueId = null;
    document.getElementById('appendBanner').hidden = true;
    document.getElementById('enqueueBtn').textContent = '加入队列';
}

async function pauseAll() {
    const res = await fetch('/api/queues/pause-all', { method: 'POST' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('操作失败: ' + body.message);
        return;
    }
    alert(`已暂停 ${body.data} 个队列`);
    loadQueues();
}

async function resumeAll() {
    const res = await fetch('/api/queues/resume-all', { method: 'POST' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('操作失败: ' + body.message);
        return;
    }
    alert(`已恢复 ${body.data} 个队列`);
    loadQueues();
}

// ---------- AI 接口管理 ----------

const aiProviderTableEl = document.getElementById('aiProviderTable');
let editingAiProviderId = null;

// 供应商模板：key 仅用于前端选择，后端统一使用 OpenAI 兼容协议（openai::Client + base_url）。
// base_url / model 参考 rig-core 0.41 各 provider 常量与官方 OpenAI 兼容端点文档（2026-07）。
const AI_PRESETS = {
    // 国际主流
    openai: { name: 'OpenAI', base_url: 'https://api.openai.com/v1', model: 'gpt-4.1-mini' },
    xai: { name: 'xAI (Grok)', base_url: 'https://api.x.ai/v1', model: 'grok-4.3' },
    groq: { name: 'Groq', base_url: 'https://api.groq.com/openai/v1', model: 'llama-3.3-70b-versatile' },
    mistral: { name: 'Mistral AI', base_url: 'https://api.mistral.ai/v1', model: 'mistral-large-3' },
    together: { name: 'Together AI', base_url: 'https://api.together.xyz/v1', model: 'meta-llama/Llama-4-Scout-17B-16E-Instruct' },
    openrouter: { name: 'OpenRouter', base_url: 'https://openrouter.ai/api/v1', model: 'openai/gpt-4.1-mini' },
    hyperbolic: { name: 'Hyperbolic', base_url: 'https://api.hyperbolic.xyz/v1', model: 'meta-llama/Llama-4-Scout-17B-16E-Instruct' },
    perplexity: { name: 'Perplexity', base_url: 'https://api.perplexity.ai', model: 'sonar' },
    gemini: { name: 'Gemini (OpenAI 兼容)', base_url: 'https://generativelanguage.googleapis.com/v1beta/openai', model: 'gemini-2.5-flash' },
    // 国内主流
    deepseek: { name: 'DeepSeek', base_url: 'https://api.deepseek.com/v1', model: 'deepseek-v4-flash' },
    moonshot: { name: 'Kimi (Moonshot 国内)', base_url: 'https://api.moonshot.cn/v1', model: 'kimi-k3' },
    moonshot_global: { name: 'Kimi (Moonshot 国际)', base_url: 'https://api.moonshot.ai/v1', model: 'kimi-k3' },
    qwen: { name: '通义千问', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen3.5-plus' },
    zhipu: { name: '智谱 AI', base_url: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4.5' },
    siliconflow: { name: 'SiliconFlow', base_url: 'https://api.siliconflow.cn/v1', model: 'deepseek-ai/DeepSeek-V4-Flash' },
    minimax: { name: 'MiniMax', base_url: 'https://api.minimaxi.com/v1', model: 'MiniMax-M3' },
    // 本地
    ollama: { name: 'Ollama (本地)', base_url: 'http://localhost:11434/v1', model: 'llama4:scout' },
    // 自定义
    custom: { name: '自定义 OpenAI 兼容', base_url: '', model: '' },
};

function applyAiPreset(key) {
    const preset = AI_PRESETS[key];
    if (!preset) return;
    if (preset.name) {
        document.getElementById('aiName').value = preset.name;
    }
    document.getElementById('aiBaseUrl').value = preset.base_url;
    document.getElementById('aiModel').value = preset.model;
}

function initAiPresetDropdown() {
    const select = document.getElementById('aiPreset');
    if (!select) return;
    // 保留第一个占位选项，其余由 AI_PRESETS 动态生成
    const placeholder = select.options[0];
    select.innerHTML = '';
    select.appendChild(placeholder);
    Object.entries(AI_PRESETS).forEach(([key, preset]) => {
        const opt = document.createElement('option');
        opt.value = key;
        opt.textContent = preset.name || key;
        select.appendChild(opt);
    });
    // 避免浏览器恢复表单状态时 select 有值而 base_url/model 仍是 HTML 默认值
    const savedValue = select.value;
    select.value = '';
    select.addEventListener('change', () => applyAiPreset(select.value));
    if (savedValue && AI_PRESETS[savedValue]) {
        select.value = savedValue;
        applyAiPreset(savedValue);
    }
}

async function loadAiStatus() {
    const res = await fetch('/api/ai/status');
    const body = await res.json();
    const banner = document.getElementById('aiStatusBanner');
    const text = document.getElementById('aiStatusText');
    if (!body.data.configured) {
        banner.hidden = false;
        text.textContent = '尚未配置 AI 接口（请在下方添加或设置 AI_API_KEY 环境变量）';
    } else {
        banner.hidden = true;
    }
}

async function loadAiProviders() {
    const res = await fetch('/api/ai/providers');
    const body = await res.json();
    const providers = body.data || [];
    if (providers.length === 0) {
        aiProviderTableEl.innerHTML = '<tr><td colspan="6" class="empty">暂无 AI 配置</td></tr>';
    } else {
        aiProviderTableEl.innerHTML = providers.map(p => `
            <tr>
                <td>${p.name}</td>
                <td>${p.base_url}</td>
                <td>${p.model}</td>
                <td>${p.api_key || '-'}</td>
                <td>${p.is_default ? '<span class="badge badge-running">默认</span>' : '-'}</td>
                <td>
                    <a href="#" onclick="editAiProvider(${p.id}); return false;">编辑</a>
                    <a href="#" onclick="testAiProvider(${p.id}); return false;">测试</a>
                    ${p.is_default ? '' : `<a href="#" onclick="setDefaultAiProvider(${p.id}); return false;">设为默认</a>`}
                    <a href="#" onclick="deleteAiProvider(${p.id}); return false;">删除</a>
                </td>
            </tr>
        `).join('');
    }
    loadAiStatus();
}

async function submitAiProvider() {
    const name = document.getElementById('aiName').value.trim();
    const base_url = document.getElementById('aiBaseUrl').value.trim();
    const api_key = document.getElementById('aiApiKey').value.trim();
    const model = document.getElementById('aiModel').value.trim();
    const timeout_secs = Number(document.getElementById('aiTimeout').value) || 60;
    if (!name || !base_url || !model) {
        alert('名称、base_url、模型名必填');
        return;
    }
    const payload = { name, base_url, api_key: api_key || null, model, timeout_secs };
    const isEdit = editingAiProviderId !== null;
    const url = isEdit ? `/api/ai/providers/${editingAiProviderId}` : '/api/ai/providers';
    const res = await fetch(url, {
        method: isEdit ? 'PUT' : 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert((isEdit ? '更新' : '添加') + '失败: ' + body.message);
        return;
    }
    resetAiProviderForm();
    loadAiProviders();
}

function editAiProvider(id) {
    const p = aiProviderTableEl.querySelectorAll('tr')[id]; // 不严谨，直接重新请求
    fetch(`/api/ai/providers/${id}`).then(r => r.json()).then(body => {
        if (body.code !== 0) return;
        const p = body.data;
        editingAiProviderId = p.id;
        document.getElementById('aiName').value = p.name;
        document.getElementById('aiBaseUrl').value = p.base_url;
        document.getElementById('aiApiKey').value = '';
        document.getElementById('aiModel').value = p.model;
        document.getElementById('aiTimeout').value = p.timeout_secs;
        document.getElementById('aiSubmitBtn').textContent = '保存修改';
        document.getElementById('aiCancelBtn').hidden = false;
    });
}

function resetAiProviderForm() {
    editingAiProviderId = null;
    document.getElementById('aiName').value = '';
    document.getElementById('aiBaseUrl').value = 'https://api.openai.com/v1';
    document.getElementById('aiApiKey').value = '';
    document.getElementById('aiModel').value = 'gpt-4o-mini';
    document.getElementById('aiTimeout').value = 60;
    document.getElementById('aiSubmitBtn').textContent = '添加配置';
    document.getElementById('aiCancelBtn').hidden = true;
}

async function testAiProvider(id) {
    const res = await fetch(`/api/ai/providers/${id}/test`, { method: 'POST' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('测试失败: ' + body.message);
        return;
    }
    alert(`连通正常，耗时 ${body.data.latency_ms} ms\n模型回复：${body.data.reply}`);
}

async function setDefaultAiProvider(id) {
    const res = await fetch(`/api/ai/providers/${id}/default`, { method: 'POST' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('设置默认失败: ' + body.message);
        return;
    }
    loadAiProviders();
}

async function deleteAiProvider(id) {
    if (!confirm('确定删除该 AI 配置？')) return;
    const res = await fetch(`/api/ai/providers/${id}`, { method: 'DELETE' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('删除失败: ' + body.message);
        return;
    }
    resetAiProviderForm();
    loadAiProviders();
}

async function loadAiCalls() {
    const res = await fetch('/api/ai/tool-calls?limit=20');
    const body = await res.json();
    const calls = body.data || [];
    const toggle = document.getElementById('aiCallsToggle');
    document.getElementById('aiCallsCount').textContent = calls.length;
    toggle.hidden = calls.length === 0;
    const wrap = document.getElementById('aiCallsTableWrap');
    if (calls.length === 0) {
        wrap.hidden = true;
        document.getElementById('aiCallsArrow').textContent = '▸';
    }
    document.getElementById('aiCallsTable').innerHTML = calls.map(c => `
        <tr>
            <td>${fmtTime(c.created_at)}</td>
            <td>${c.tool_name}</td>
            <td title="${c.arguments}">${c.arguments.slice(0, 60)}${c.arguments.length > 60 ? '...' : ''}</td>
            <td title="${c.result || c.error || ''}">${c.result ? '成功' : (c.error ? '失败: ' + c.error.slice(0, 40) : '-')}</td>
            <td>${c.duration_ms} ms</td>
        </tr>
    `).join('');
}

function toggleAiCalls() {
    const wrap = document.getElementById('aiCallsTableWrap');
    wrap.hidden = !wrap.hidden;
    document.getElementById('aiCallsArrow').textContent = wrap.hidden ? '▸' : '▾';
}

document.getElementById('previewBtn').addEventListener('click', previewSelector);
document.getElementById('enqueueBtn').addEventListener('click', enqueueBySelector);
document.getElementById('crawlSelectedBtn').addEventListener('click', crawlSelected);
document.getElementById('pauseAllBtn').addEventListener('click', pauseAll);
document.getElementById('resumeAllBtn').addEventListener('click', resumeAll);
document.getElementById('aiSubmitBtn').addEventListener('click', submitAiProvider);
document.getElementById('aiCancelBtn').addEventListener('click', resetAiProviderForm);

initAiPresetDropdown();
checkHealth();
loadTags().then(loadProducts);
loadItems();
loadQueues();
loadAiProviders();
loadAiCalls();
