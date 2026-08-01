import { useState } from 'react';
import { Button, Card, Input, Table, Typography } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query';
import { PageHeader } from '@/components/PageHeader';
import { apiGet, fmtTime } from '@/lib/api';
import type { Item, PageResponse } from '@/types/api';

interface ItemsQuery {
  page: number;
  pageSize: number;
  search: string;
}

export function ItemsPage() {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState<ItemsQuery>({ page: 1, pageSize: 20, search: '' });

  const params = new URLSearchParams({ page: String(query.page), page_size: String(query.pageSize) });
  if (query.search) params.set('search', query.search);

  const { data, isLoading, isFetching } = useQuery({
    queryKey: ['items', query],
    queryFn: () => apiGet<PageResponse<Item>>(`/api/items?${params}`),
    placeholderData: keepPreviousData,
  });

  return (
    <div>
      <PageHeader
        title="抓取数据"
        description="队列执行后抓到的闲鱼原始数据"
        extra={
          <>
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
          </>
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
