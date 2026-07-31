const statusEl = document.getElementById('status');
const tableEl = document.getElementById('itemTable');
const tagTableEl = document.getElementById('tagTable');
let editingTagId = null;

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
    renderTags(body.data);
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
    loadTags();
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
    loadTags();
}

async function deleteTag(id) {
    if (!confirm('确定删除该标签？')) return;
    const res = await fetch(`/api/tags/${id}`, { method: 'DELETE' });
    const body = await res.json();
    if (body.code !== 0) {
        alert('删除失败: ' + body.message);
    }
    loadTags();
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

checkHealth();
loadTags();
loadItems();
