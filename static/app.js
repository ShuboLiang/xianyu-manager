const statusEl = document.getElementById('status');
const tableEl = document.getElementById('itemTable');

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

checkHealth();
loadItems();
