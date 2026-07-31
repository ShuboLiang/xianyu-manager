const statusEl = document.getElementById('status');
const tableEl = document.getElementById('itemTable');
const tagTableEl = document.getElementById('tagTable');
const productTableEl = document.getElementById('productTable');
let editingTagId = null;
let editingProductId = null;
let cachedTags = [];

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

async function startCrawl() {
    const keyword = document.getElementById('keyword').value.trim();
    const maxPages = Number(document.getElementById('maxPages').value) || 1;
    if (!keyword) {
        alert('请输入关键词');
        return;
    }
    const res = await fetch('/api/crawl', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ keyword, max_pages: maxPages }),
    });
    const body = await res.json();
    if (body.code !== 0) {
        alert('创建任务失败: ' + body.message);
        return;
    }
    pollTask(body.data.id);
}

// 轮询任务状态直到 done / failed，然后刷新列表
async function pollTask(taskId) {
    const res = await fetch(`/api/crawl/${taskId}`);
    const body = await res.json();
    if (body.code !== 0) {
        alert('查询任务失败: ' + body.message);
        return;
    }
    const task = body.data;
    if (task.status === 'done') {
        loadItems();
    } else if (task.status === 'failed') {
        alert('抓取失败: ' + (task.error || '未知错误'));
    } else {
        setTimeout(() => pollTask(taskId), 500);
    }
}

document.getElementById('crawlBtn').addEventListener('click', startCrawl);
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
        productTableEl.innerHTML = '<tr><td colspan="9" class="empty">暂无商品</td></tr>';
        return;
    }
    productTableEl.innerHTML = products.map(p => `
        <tr>
            <td>${p.name}</td>
            <td>${p.tag_names.length ? p.tag_names.join('、') : '<span class="muted">无标签</span>'}</td>
            <td>${fmtPrice(p.median_price)}</td>
            <td>${fmtPrice(p.avg_price)}</td>
            <td>${p.crawled_count ?? '-'}</td>
            <td>${fmtTime(p.last_crawled_at)}</td>
            <td>${fmtPrice(p.recycle_price)}</td>
            <td>${p.remark || '-'}</td>
            <td>
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

checkHealth();
loadTags().then(loadProducts);
loadItems();
