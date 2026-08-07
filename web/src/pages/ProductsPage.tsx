import { useEffect, useRef, useState } from 'react';
import {
  App as AntApp,
  Alert,
  Button,
  Card,
  Dropdown,
  Form,
  Input,
  InputNumber,
  Modal,
  Progress,
  Select,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType, SorterResult } from 'antd/es/table/interface';
import { DownOutlined, EditOutlined, PlusOutlined } from '@ant-design/icons';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query';
import { PageHeader } from '@/components/PageHeader';
import { apiDelete, apiGet, apiPost, apiPut, fmtPrice, fmtTime } from '@/lib/api';
import { useQueue } from '@/lib/queue';
import { useTags } from '@/lib/queries';
import type {
  ClassifyProductsResponse,
  ClassifyTask,
  Item,
  PageResponse,
  Product,
  ProductBatchCreateResponse,
  ProductBatchDeleteIdsPreviewResponse,
  ProductBatchDeleteResponse,
} from '@/types/api';

type SortKey = 'median_price' | 'avg_price' | 'mode_price' | 'crawled_count' | 'last_crawled_at' | 'recycle_price';

interface ProductsQuery {
  page: number;
  pageSize: number;
  sortBy: SortKey | null;
  sortDir: 'asc' | 'desc';
  search: string;
  tagId: number | null;
}

const TERMINAL_TASK_STATUS = ['done', 'failed', 'cancelled'];

// 分类任务状态 → 中文（后端为英文字符串）
const CLASSIFY_STATUS_TEXT: Record<string, string> = {
  pending: '等待中',
  running: '运行中',
  done: '已完成',
  failed: '失败',
  cancelled: '已取消',
};

// 标签配色：按 id 取色板，保证同一标签颜色稳定
const TAG_COLORS = ['blue', 'green', 'orange', 'purple', 'cyan', 'magenta', 'geekblue', 'volcano'];
const tagColor = (id: number) => TAG_COLORS[id % TAG_COLORS.length];

export function ProductsPage() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();
  const { data: tags = [] } = useTags();
  const { enqueue } = useQueue();

  const [query, setQuery] = useState<ProductsQuery>({
    page: 1,
    pageSize: 20,
    sortBy: null,
    sortDir: 'desc',
    search: '',
    tagId: null,
  });

  const params = new URLSearchParams({ page: String(query.page), page_size: String(query.pageSize) });
  if (query.sortBy) {
    params.set('sort_by', query.sortBy);
    params.set('sort_dir', query.sortDir);
  }
  if (query.search) params.set('search', query.search);
  if (query.tagId !== null) params.set('tag_id', String(query.tagId));

  const { data, isLoading } = useQuery({
    queryKey: ['products', query],
    queryFn: () => apiGet<PageResponse<Product>>(`/api/products?${params}`),
    placeholderData: keepPreviousData,
  });

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['products'] });
    queryClient.invalidateQueries({ queryKey: ['aiCalls'] });
  };

  // 跨页选择：用 Set 记录所有已选 id，翻页/排序不清空，搜索/标签筛选变更时清空
  const checkedSetRef = useRef(new Set<number>());
  const [checkedCount, setCheckedCount] = useState(0);
  const addChecked = (id: number) => { checkedSetRef.current.add(id); setCheckedCount(checkedSetRef.current.size); };
  const removeChecked = (id: number) => { checkedSetRef.current.delete(id); setCheckedCount(checkedSetRef.current.size); };
  const clearChecked = () => { checkedSetRef.current.clear(); setCheckedCount(0); };
  const getCheckedIds = () => Array.from(checkedSetRef.current);

  // ---------- 新建 / 编辑（弹窗表单） ----------
  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form] = Form.useForm<{ name: string; tag_ids: number[]; remark?: string }>();

  const openCreate = () => {
    setEditingId(null);
    form.resetFields();
    setFormOpen(true);
  };

  const openEdit = async (id: number) => {
    try {
      const p = await apiGet<Product>(`/api/products/${id}`);
      setEditingId(p.id);
      form.setFieldsValue({ name: p.name, tag_ids: p.tag_ids, remark: p.remark ?? undefined });
      setFormOpen(true);
    } catch (e) {
      message.error(`加载商品失败: ${(e as Error).message}`);
    }
  };

  const submitForm = async () => {
    const values = await form.validateFields();
    const payload = {
      name: values.name.trim(),
      tag_ids: values.tag_ids ?? [],
      remark: values.remark?.trim() || null,
    };
    const isEdit = editingId !== null;
    try {
      if (isEdit) {
        await apiPut(`/api/products/${editingId}`, payload);
      } else {
        await apiPost('/api/products', payload);
      }
      message.success(isEdit ? '已保存修改' : '已添加商品');
      setFormOpen(false);
      refresh();
    } catch (e) {
      message.error(`${isEdit ? '更新' : '创建'}失败: ${(e as Error).message}`);
    }
  };

  const confirmDelete = (p: Product) => {
    modal.confirm({
      title: `删除商品「${p.name}」？`,
      content: '删除后其标签关联与抓取统计将一并清除；若该商品在活跃队列中，对应条目会被跳过。此操作不可恢复。',
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await apiDelete(`/api/products/${p.id}`);
          message.success('已删除');
          refresh();
        } catch (e) {
          message.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  // ---------- 回收价行内编辑 ----------
  const [recycleEditingId, setRecycleEditingId] = useState<number | null>(null);
  const [recycleValue, setRecycleValue] = useState<number | null>(null);

  const saveRecyclePrice = async (id: number) => {
    // 留空 = 清空回收价；否则必须是正数
    if (recycleValue !== null && recycleValue <= 0) {
      message.error('回收价必须为正数，留空表示清空');
      return;
    }
    try {
      await apiPut(`/api/products/${id}`, { recycle_price: recycleValue });
      setRecycleEditingId(null);
      refresh();
    } catch (e) {
      message.error(`保存回收价失败: ${(e as Error).message}`);
    }
  };

  // ---------- 抓取明细弹窗 ----------
  const [detailProduct, setDetailProduct] = useState<Product | null>(null);
  const [detailItems, setDetailItems] = useState<Item[] | null>(null);

  const openDetail = async (p: Product) => {
    setDetailProduct(p);
    setDetailItems(null);
    try {
      setDetailItems(await apiGet<Item[]>(`/api/products/${p.id}/latest-items`));
    } catch (e) {
      message.error(`加载抓取明细失败: ${(e as Error).message}`);
      setDetailProduct(null);
    }
  };

  // ---------- 批量导入 ----------
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchText, setBatchText] = useState('');
  const [batchTagIds, setBatchTagIds] = useState<number[]>([]);
  const [batchResult, setBatchResult] = useState<string | null>(null);
  const [batchSubmitting, setBatchSubmitting] = useState(false);

  const submitBatch = async () => {
    const names = batchText
      .split(/\n/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (names.length === 0) {
      message.error('请输入至少一个商品名');
      return;
    }
    if (names.length > 1000) {
      message.error(`最多 1000 条，当前 ${names.length} 条`);
      return;
    }
    setBatchSubmitting(true);
    try {
      const resp = await apiPost<ProductBatchCreateResponse>('/api/products/batch', {
        names,
        tag_ids: batchTagIds.length ? batchTagIds : null,
      });
      let msg = `创建 ${resp.created.length} 条`;
      if (resp.created.length) msg += '：' + resp.created.map((p) => p.name).join('、');
      if (resp.skipped.length) {
        msg += `；跳过 ${resp.skipped.length} 条：` + resp.skipped.map((s) => `${s.name}（${s.reason}）`).join('、');
      }
      setBatchResult(msg);
      if (resp.created.length > 0) {
        setBatchText('');
        refresh();
      }
    } catch (e) {
      message.error(`导入失败: ${(e as Error).message}`);
    } finally {
      setBatchSubmitting(false);
    }
  };

  // ---------- AI 自动打标签 ----------
  const [classifying, setClassifying] = useState(false);
  const [classifyTask, setClassifyTask] = useState<ClassifyTask | null>(null);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const stopPolling = () => {
    if (pollTimer.current) {
      clearTimeout(pollTimer.current);
      pollTimer.current = null;
    }
  };
  useEffect(() => () => stopPolling(), []);

  const aiClassify = async () => {
    const ids = getCheckedIds();
    if (ids.length === 0) {
      message.error('请先勾选商品');
      return;
    }
    if (ids.length <= 50) {
      setClassifying(true);
      try {
        const resp = await apiPost<ClassifyProductsResponse>('/api/ai/classify-products', {
          product_ids: ids,
        });
        let msg = `AI 打标签完成，涉及 ${resp.suggestions.length} 个商品`;
        if (resp.warnings.length) msg += `\n${resp.warnings.length} 条警告：\n${resp.warnings.join('\n')}`;
        message.success(<div style={{ whiteSpace: 'pre-line' }}>{msg}</div>);
        refresh();
      } catch (e) {
        message.error(`AI 分类失败: ${(e as Error).message}`);
      } finally {
        setClassifying(false);
      }
    } else {
      try {
        const task = await apiPost<ClassifyTask>('/api/ai/classify-tasks', { product_ids: ids });
        setClassifyTask(task);
        pollClassify(task.id);
      } catch (e) {
        message.error(`创建分类任务失败: ${(e as Error).message}`);
      }
    }
  };

  const pollClassify = (taskId: string) => {
    stopPolling();
    const tick = async () => {
      try {
        const task = await apiGet<ClassifyTask>(`/api/ai/classify-tasks/${taskId}`);
        setClassifyTask(task);
        if (TERMINAL_TASK_STATUS.includes(task.status)) {
          refresh();
          setTimeout(() => setClassifyTask(null), 3000);
          return;
        }
        pollTimer.current = setTimeout(tick, 2000);
      } catch (e) {
        setClassifyTask((prev) => (prev ? { ...prev, status: 'failed', error: (e as Error).message } : prev));
      }
    };
    void tick();
  };

  const cancelClassify = async () => {
    if (!classifyTask) return;
    try {
      const task = await apiPost<ClassifyTask>(`/api/ai/classify-tasks/${classifyTask.id}/cancel`);
      setClassifyTask(task);
      stopPolling();
      refresh();
    } catch (e) {
      message.error(`取消失败: ${(e as Error).message}`);
    }
  };

  const classifyPct =
    classifyTask && classifyTask.total > 0 ? Math.round((classifyTask.processed / classifyTask.total) * 100) : 0;

  // ---------- 批量入队 / 批量删除 / 导出 ----------
  const crawlSelected = async () => {
    const ids = getCheckedIds();
    if (ids.length === 0) {
      message.error('请先勾选商品');
      return;
    }
    const ok = await enqueue({ product_ids: ids });
    if (ok) clearChecked();
  };

  const batchDelete = async () => {
    const ids = getCheckedIds();
    if (ids.length === 0) {
      message.error('请先勾选商品');
      return;
    }
    // 先预览：实际存在的商品数 + 名称样本 + 活跃队列占用（仅提示，不阻止）
    let preview: ProductBatchDeleteIdsPreviewResponse;
    try {
      preview = await apiPost<ProductBatchDeleteIdsPreviewResponse>(
        '/api/products/batch-delete-ids/preview',
        { ids },
      );
    } catch (e) {
      message.error(`预览失败: ${(e as Error).message}`);
      return;
    }
    if (preview.total === 0) {
      message.info('勾选的商品已不存在');
      clearChecked();
      return;
    }
    modal.confirm({
      title: `批量删除 ${preview.total} 个商品`,
      content: (
        <div>
          <p>删除后标签关联与抓取统计将一并清除，已抓取的历史数据保留：</p>
          <ul style={{ maxHeight: 160, overflowY: 'auto', paddingLeft: 18, margin: 0 }}>
            {preview.sample.map((n, i) => (
              <li key={i}>{n}</li>
            ))}
            {preview.total > preview.sample.length && <li>… 等 {preview.total} 个</li>}
          </ul>
          {preview.in_active_queues > 0 && (
            <Typography.Text type="warning">
              其中 {preview.in_active_queues} 个商品在活跃队列中，轮到时会自动跳过。
            </Typography.Text>
          )}
        </div>
      ),
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          const res = await apiPost<ProductBatchDeleteResponse>(
            '/api/products/batch-delete-ids',
            { ids },
          );
          message.success(`已删除 ${res.deleted} 个商品`);
          clearChecked();
          refresh();
          queryClient.invalidateQueries({ queryKey: ['stats'] });
        } catch (e) {
          message.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  const exportExcel = async () => {
    try {
      const res = await fetch('/api/products/export');
      if (!res.ok) throw new Error(await res.text());
      const blob = await res.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'products.xlsx';
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      message.error(`导出失败: ${(e as Error).message}`);
    }
  };

  // ---------- 表格 ----------
  const sortOrderOf = (key: SortKey) =>
    query.sortBy === key ? (query.sortDir === 'asc' ? ('ascend' as const) : ('descend' as const)) : null;

  const columns: ColumnsType<Product> = [
    { title: '商品名', dataIndex: 'name', width: 160, ellipsis: true },
    {
      title: '标签',
      key: 'tags',
      width: 145,
      render: (_, p) =>
        p.tag_names.length ? (
          <Space size={4} wrap>
            {p.tag_names.map((n, i) => (
              <Tag key={n} color={tagColor(p.tag_ids[i] ?? 0)} style={{ marginInlineEnd: 0 }}>
                {n}
              </Tag>
            ))}
          </Space>
        ) : (
          <Typography.Text type="secondary">无标签</Typography.Text>
        ),
    },
    {
      title: '中位数',
      dataIndex: 'median_price',
      sorter: true,
      sortOrder: sortOrderOf('median_price'),
      align: 'right',
      width: 100,
      render: (v: number | null) => <span className="num">{fmtPrice(v)}</span>,
    },
    {
      title: '均价',
      dataIndex: 'avg_price',
      sorter: true,
      sortOrder: sortOrderOf('avg_price'),
      align: 'right',
      width: 100,
      render: (v: number | null) => <span className="num">{fmtPrice(v)}</span>,
    },
    {
      title: '常见价位',
      dataIndex: 'mode_price',
      sorter: true,
      sortOrder: sortOrderOf('mode_price'),
      align: 'right',
      width: 120,
      render: (v: number | null) => {
        // 档宽与后端 mode_bucket_width 规则一致：按价格量级自适应
        const w = v === null ? 0 : v < 100 ? 10 : v < 1000 ? 50 : v < 10000 ? 100 : 500;
        return (
          <span className="num" title="分档众数：商品数最多的价格区间（档宽随价格量级自适应）">
            {v === null ? '-' : `¥${v}–${v + w}`}
          </span>
        );
      },
    },
    {
      title: '爬取数量',
      dataIndex: 'crawled_count',
      sorter: true,
      sortOrder: sortOrderOf('crawled_count'),
      align: 'right',
      width: 90,
      render: (v: number | null) => <span className="num">{v ?? '-'}</span>,
    },
    {
      title: '最后爬取',
      dataIndex: 'last_crawled_at',
      sorter: true,
      sortOrder: sortOrderOf('last_crawled_at'),
      width: 145,
      render: (v: number | null) => (
        <span className="num" style={{ fontSize: 12 }}>
          {fmtTime(v)}
        </span>
      ),
    },
    {
      title: '回收价',
      dataIndex: 'recycle_price',
      sorter: true,
      sortOrder: sortOrderOf('recycle_price'),
      align: 'right',
      width: 135,
      render: (v: number | null, p) =>
        recycleEditingId === p.id ? (
          <Space.Compact size="small">
            <InputNumber
              size="small"
              min={0}
              step={0.01}
              placeholder="留空即清空"
              autoFocus
              value={recycleValue}
              onChange={(val) => setRecycleValue(val)}
              onPressEnter={() => void saveRecyclePrice(p.id)}
              onKeyDown={(e) => {
                if (e.key === 'Escape') setRecycleEditingId(null);
              }}
              style={{ width: 110 }}
            />
            <Button size="small" type="primary" onClick={() => void saveRecyclePrice(p.id)}>
              保存
            </Button>
          </Space.Compact>
        ) : (
          <Space size={4}>
            <span className="num">{fmtPrice(v)}</span>
            <Button
              type="text"
              size="small"
              icon={<EditOutlined />}
              title="手动修改回收价（下一轮爬取会覆盖）"
              onClick={() => {
                setRecycleEditingId(p.id);
                setRecycleValue(v);
              }}
            />
          </Space>
        ),
    },
    {
      title: '备注',
      dataIndex: 'remark',
      width: 150,
      ellipsis: true,
      render: (v: string | null) => v || <Typography.Text type="secondary">-</Typography.Text>,
    },
    {
      title: '操作',
      key: 'actions',
      width: 180,
      render: (_, p) => (
        <Space split={<Typography.Text type="secondary">|</Typography.Text>} size={2}>
          <Button type="link" size="small" style={{ padding: 0 }} onClick={() => void enqueue({ product_ids: [p.id] })}>
            抓取
          </Button>
          <Button type="link" size="small" style={{ padding: 0 }} onClick={() => void openDetail(p)}>
            明细
          </Button>
          <Button type="link" size="small" style={{ padding: 0 }} onClick={() => void openEdit(p.id)}>
            编辑
          </Button>
          <Button type="link" size="small" danger style={{ padding: 0 }} onClick={() => confirmDelete(p)}>
            删除
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <PageHeader
        title="商品管理"
        description="管理要爬取的商品，勾选后可批量入队或交给 AI 打标签"
        extra={
          <>
            <Dropdown
              menu={{
                items: [
                  { key: 'import', label: '批量导入' },
                  { key: 'export', label: '导出 Excel' },
                ],
                onClick: ({ key }) => {
                  if (key === 'import') {
                    setBatchResult(null);
                    setBatchOpen(true);
                  } else {
                    void exportExcel();
                  }
                },
              }}
            >
              <Button>
                更多 <DownOutlined />
              </Button>
            </Dropdown>
            <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
              新建商品
            </Button>
          </>
        }
      />

      <Card>
        <Space direction="vertical" size={12} style={{ width: '100%' }}>
          <Space wrap size={12}>
            <Input.Search
              allowClear
              placeholder="搜索商品名"
              style={{ width: 280 }}
              onSearch={(v) => {
                clearChecked();
                setQuery((q) => ({ ...q, page: 1, search: v.trim() }));
              }}
            />
            <Select
              allowClear
              placeholder="按标签筛选"
              style={{ minWidth: 160 }}
              value={query.tagId}
              onChange={(v) => {
                clearChecked();
                setQuery((q) => ({ ...q, page: 1, tagId: v ?? null }));
              }}
              options={[
                { label: '无标签', value: -1 },
                ...tags.map((t) => ({ label: t.name, value: t.id })),
              ]}
            />
            {checkedCount > 0 && (
              <>
                <Typography.Text type="secondary">已选 {checkedCount} 项</Typography.Text>
                <Button onClick={crawlSelected}>加入队列</Button>
                <Button loading={classifying} onClick={aiClassify}>
                  AI 打标签
                </Button>
                <Button danger onClick={batchDelete}>批量删除</Button>
                <Button type="text" onClick={clearChecked}>清除</Button>
              </>
            )}
          </Space>

          {classifyTask && (
            <Alert
              type={
                classifyTask.status === 'failed'
                  ? 'error'
                  : TERMINAL_TASK_STATUS.includes(classifyTask.status)
                    ? 'success'
                    : 'info'
              }
              message={
                <Space size={12} style={{ width: '100%' }}>
                  <Progress percent={classifyPct} size="small" style={{ width: 200, margin: 0 }} />
                  <span className="num" style={{ fontSize: 12 }}>
                    AI 打标签：{classifyTask.processed}/{classifyTask.total}
                    {classifyTask.failed > 0 && `，失败 ${classifyTask.failed}`} ·{' '}
                    {CLASSIFY_STATUS_TEXT[classifyTask.status] ?? classifyTask.status}
                    {classifyTask.error && ` · ${classifyTask.error}`}
                  </span>
                  {!TERMINAL_TASK_STATUS.includes(classifyTask.status) && (
                    <Button size="small" onClick={cancelClassify}>
                      取消
                    </Button>
                  )}
                </Space>
              }
            />
          )}

          <Table<Product>
            rowKey="id"
            size="middle"
            loading={isLoading}
            columns={columns}
            dataSource={data?.items ?? []}
            rowSelection={{
              selectedRowKeys: (data?.items ?? []).filter((p) => checkedSetRef.current.has(p.id)).map((p) => p.id),
              onSelect: (record, selected) => {
                if (selected) {
                  addChecked(record.id);
                } else {
                  removeChecked(record.id);
                }
              },
              onSelectAll: (selected, _selectedRows, changeRows) => {
                for (const r of changeRows) {
                  if (selected) {
                    checkedSetRef.current.add(r.id);
                  } else {
                    checkedSetRef.current.delete(r.id);
                  }
                }
                setCheckedCount(checkedSetRef.current.size);
              },
            }}
            onChange={(pagination, _filters, sorter) => {
              const s = sorter as SorterResult<Product>;
              const nextSortBy = s.order ? ((s.field as SortKey) ?? null) : null;
              const nextSortDir: 'asc' | 'desc' = s.order === 'ascend' ? 'asc' : 'desc';
              setQuery((q) => {
                // 排序变更回到第 1 页（纯翻页保持当前页）
                const sortChanged = nextSortBy !== q.sortBy || nextSortDir !== q.sortDir;
                return {
                  ...q,
                  page: sortChanged ? 1 : (pagination.current ?? 1),
                  pageSize: pagination.pageSize ?? 20,
                  sortBy: nextSortBy,
                  sortDir: nextSortDir,
                };
              });
            }}
            pagination={{
              current: query.page,
              pageSize: query.pageSize,
              total: data?.total ?? 0,
              showSizeChanger: true,
              showTotal: (t) => `共 ${t} 条`,
            }}
            scroll={{ x: 1_380 }}
          />
        </Space>
      </Card>

      {/* 新建 / 编辑商品 */}
      <Modal
        title={editingId !== null ? '编辑商品' : '新建商品'}
        open={formOpen}
        onOk={submitForm}
        onCancel={() => setFormOpen(false)}
        okText={editingId !== null ? '保存修改' : '添加商品'}
        cancelText="取消"
        destroyOnHidden
      >
        <Form form={form} layout="vertical" initialValues={{ tag_ids: [] }}>
          <Form.Item
            name="name"
            label="商品名"
            rules={[{ required: true, whitespace: true, message: '商品名不能为空' }]}
          >
            <Input placeholder="如：佳能 5D4 机身" />
          </Form.Item>
          <Form.Item name="tag_ids" label="标签">
            <Select
              mode="multiple"
              allowClear
              placeholder="选择标签（可多选）"
              options={tags.map((t) => ({ label: t.name, value: t.id }))}
            />
          </Form.Item>
          <Form.Item name="remark" label="备注">
            <Input.TextArea rows={2} placeholder="可选" />
          </Form.Item>
        </Form>
      </Modal>

      {/* 批量导入 */}
      <Modal
        title="批量导入商品"
        open={batchOpen}
        onCancel={() => setBatchOpen(false)}
        footer={
          <Space>
            <Button onClick={() => setBatchOpen(false)}>关闭</Button>
            <Button type="primary" loading={batchSubmitting} onClick={submitBatch}>
              提交导入
            </Button>
          </Space>
        }
      >
        <Space direction="vertical" size={12} style={{ width: '100%' }}>
          <Typography.Text type="secondary">每行一个商品名，最多 1000 条</Typography.Text>
          <Input.TextArea
            rows={8}
            placeholder={'佳能 5D4\n索尼 A7M3 机身\n富士 X-T5 套机'}
            value={batchText}
            onChange={(e) => setBatchText(e.target.value)}
          />
          <div>
            <div style={{ marginBottom: 4 }}>
              <Typography.Text type="secondary">统一标签（可选）：</Typography.Text>
            </div>
            <Select
              mode="multiple"
              allowClear
              style={{ width: '100%' }}
              placeholder="为本次导入的商品统一打标签"
              options={tags.map((t) => ({ label: t.name, value: t.id }))}
              value={batchTagIds}
              onChange={setBatchTagIds}
            />
          </div>
          {batchResult && <Alert type="info" message={batchResult} />}
        </Space>
      </Modal>

      {/* 最新一轮抓取明细 */}
      <Modal
        title={
          <>
            「{detailProduct?.name}」最新一轮抓取明细
            {detailItems && (
              <Typography.Text type="secondary" style={{ marginLeft: 8, fontSize: 13, fontWeight: 400 }}>
                共 {detailItems.length} 条
              </Typography.Text>
            )}
          </>
        }
        open={detailProduct !== null}
        onCancel={() => setDetailProduct(null)}
        footer={null}
        width={920}
      >
        <Table<Item>
          rowKey="id"
          size="small"
          loading={detailItems === null}
          dataSource={detailItems ?? []}
          scroll={{ y: '60vh' }}
          locale={{ emptyText: '暂无抓取明细——该商品还没有完成过一轮抓取，点「抓取」开始。' }}
          columns={[
            {
              title: '标题',
              dataIndex: 'title',
              // 明细场景需要读全标题，最多两行，悬浮见全文
              render: (v: string) => (
                <span
                  title={v}
                  style={{
                    display: '-webkit-box',
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: 'vertical',
                    overflow: 'hidden',
                  }}
                >
                  {v}
                </span>
              ),
            },
            {
              title: '价格',
              dataIndex: 'price',
              width: 110,
              align: 'right',
              sorter: (a, b) => a.price - b.price,
              render: (v: number) => <span className="num">¥{v}</span>,
            },
            {
              title: '卖家',
              dataIndex: 'seller',
              width: 140,
              ellipsis: true,
              render: (v: string) => v || <Typography.Text type="secondary">-</Typography.Text>,
            },
            {
              title: '链接',
              key: 'url',
              width: 70,
              render: (_, it) => (
                <Typography.Link href={it.url} target="_blank" rel="noreferrer">
                  查看
                </Typography.Link>
              ),
            },
          ]}
          pagination={false}
        />
      </Modal>
    </div>
  );
}
