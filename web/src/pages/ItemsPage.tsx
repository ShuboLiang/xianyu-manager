import { useState } from 'react';
import { App as AntApp, Button, Card, Input, Space, Table, Typography } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query';
import { PageHeader } from '@/components/PageHeader';
import { apiDelete, apiGet, apiPost, fmtTime } from '@/lib/api';
import type {
  Item,
  ItemBatchDeletePreviewResponse,
  ItemBatchDeleteResponse,
  PageResponse,
} from '@/types/api';

interface ItemsQuery {
  page: number;
  pageSize: number;
  search: string;
}

export function ItemsPage() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();
  const [query, setQuery] = useState<ItemsQuery>({ page: 1, pageSize: 20, search: '' });

  const params = new URLSearchParams({ page: String(query.page), page_size: String(query.pageSize) });
  if (query.search) params.set('search', query.search);

  const { data, isLoading, isFetching } = useQuery({
    queryKey: ['items', query],
    queryFn: () => apiGet<PageResponse<Item>>(`/api/items?${params}`),
    placeholderData: keepPreviousData,
  });

  const onChanged = () => {
    queryClient.invalidateQueries({ queryKey: ['items'] });
    queryClient.invalidateQueries({ queryKey: ['stats'] });
  };

  const removeOne = (it: Item) => {
    modal.confirm({
      title: '删除这条抓取记录？',
      content: `「${it.title}」删除后不可恢复，价格趋势图中对应数据点也会消失。`,
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await apiDelete(`/api/items/${encodeURIComponent(it.id)}`);
          message.success('已删除');
          onChanged();
        } catch (e) {
          message.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  const removeMatching = async () => {
    // 先预览命中数量与样本，确认后执行；search 与列表搜索同一语义，空 = 清空全部
    let preview: ItemBatchDeletePreviewResponse;
    try {
      preview = await apiPost<ItemBatchDeletePreviewResponse>('/api/items/batch-delete/preview', {
        search: query.search || null,
      });
    } catch (e) {
      message.error(`预览失败: ${(e as Error).message}`);
      return;
    }
    if (preview.total === 0) {
      message.info('没有匹配的抓取记录');
      return;
    }
    modal.confirm({
      title: query.search
        ? `删除搜索「${query.search}」匹配的全部记录？`
        : '清空全部抓取数据？',
      content: (
        <div>
          <p>
            将删除 {preview.total} 条抓取记录
            {!query.search && <Typography.Text type="danger">（当前无搜索条件，即全部数据）</Typography.Text>}
            ，价格趋势图中对应数据点也会消失：
          </p>
          <ul style={{ maxHeight: 160, overflowY: 'auto', paddingLeft: 18, margin: 0 }}>
            {preview.sample.map((t, i) => (
              <li key={i}>{t}</li>
            ))}
            {preview.total > preview.sample.length && <li>… 等 {preview.total} 条</li>}
          </ul>
        </div>
      ),
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          const res = await apiPost<ItemBatchDeleteResponse>('/api/items/batch-delete', {
            search: query.search || null,
          });
          message.success(`已删除 ${res.deleted} 条抓取记录`);
          setQuery((q) => ({ ...q, page: 1 }));
          onChanged();
        } catch (e) {
          message.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  return (
    <div>
      <PageHeader
        title="抓取数据"
        description="队列执行后抓到的闲鱼原始数据"
        extra={
          <Space>
            <Input.Search
              allowClear
              placeholder="搜索标题 / 商品名"
              style={{ width: 280 }}
              onSearch={(v) => setQuery((q) => ({ ...q, page: 1, search: v.trim() }))}
            />
            <Button
              icon={<ReloadOutlined />}
              loading={isFetching}
              onClick={() => queryClient.invalidateQueries({ queryKey: ['items'] })}
            >
              刷新列表
            </Button>
            <Button danger onClick={removeMatching}>
              {query.search ? '删除搜索结果' : '清空全部'}
            </Button>
          </Space>
        }
      />
      <Card>
        <Table<Item>
          rowKey="id"
          loading={isLoading}
          dataSource={data?.items ?? []}
          locale={{
            emptyText: query.search
              ? '未找到匹配的商品名或标题，请尝试其他关键词。'
              : '暂无抓取数据——队列执行后，抓到的原始数据会出现在这里。',
          }}
          columns={[
            {
              title: '商品名',
              dataIndex: 'product_name',
              width: 140,
              ellipsis: true,
              render: (v: string | null) => v || <Typography.Text type="secondary">-</Typography.Text>,
            },
            {
              title: '标题',
              dataIndex: 'title',
              // 闲鱼标题信息量大，最多展示两行，悬浮可见全文
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
              render: (v: number) => <span className="num">¥{v}</span>,
            },
            { title: '卖家', dataIndex: 'seller', width: 140, ellipsis: true },
            {
              title: '抓取时间',
              dataIndex: 'crawled_at',
              width: 170,
              render: (v: number) => (
                <span className="num" style={{ fontSize: 12 }}>
                  {fmtTime(v)}
                </span>
              ),
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
            {
              title: '操作',
              key: 'actions',
              width: 70,
              render: (_, it) => (
                <Button type="link" size="small" danger style={{ padding: 0 }} onClick={() => removeOne(it)}>
                  删除
                </Button>
              ),
            },
          ]}
          onChange={(pagination) =>
            setQuery((q) => ({
              ...q,
              page: pagination.current ?? 1,
              pageSize: pagination.pageSize ?? 20,
            }))
          }
          pagination={{
            current: query.page,
            pageSize: query.pageSize,
            total: data?.total ?? 0,
            showSizeChanger: true,
            showTotal: (t) => `共 ${t} 条`,
          }}
        />
      </Card>
    </div>
  );
}
