import { useState } from 'react';
import { App as AntApp, Button, Card, Empty, Select, Space, Spin, Tag, theme as antdTheme } from 'antd';
import { useQuery } from '@tanstack/react-query';
import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { PageHeader } from '@/components/PageHeader';
import { apiGet } from '@/lib/api';
import type { PageResponse, PriceTrendSeries, Product } from '@/types/api';

const COLORS = [
  '#3b82f6', '#ef4444', '#22c55e', '#f59e0b', '#8b5cf6',
  '#ec4899', '#06b6d4', '#f97316', '#6366f1', '#14b8a6',
];

export function TrendsPage() {
  const { message } = AntApp.useApp();
  // recharts 不感知 antd 主题，坐标轴/网格/提示框颜色需要手动接 token，否则深色模式下对比度不足
  const { token } = antdTheme.useToken();
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [series, setSeries] = useState<PriceTrendSeries[]>([]);
  const [chartLoading, setChartLoading] = useState(false);

  const { data: allProducts = [], isLoading: productsLoading } = useQuery({
    queryKey: ['products', 'all-for-trends'],
    queryFn: async () => (await apiGet<PageResponse<Product>>('/api/products?page=1&page_size=1000')).items,
  });

  const loadTrend = async () => {
    if (selectedIds.length === 0) {
      message.error('请先选择商品');
      return;
    }
    setChartLoading(true);
    try {
      setSeries(await apiGet<PriceTrendSeries[]>(`/api/products/price-trend?product_ids=${selectedIds.join(',')}`));
    } catch (e) {
      message.error(`加载趋势数据失败: ${(e as Error).message}`);
    } finally {
      setChartLoading(false);
    }
  };

  // 合并多条 series 为 recharts 所需格式：同一时间戳合并为一行
  const allTss = [...new Set(series.flatMap((s) => s.points.map((p) => p.crawled_at)))].sort();
  const chartData = allTss.map((ts) => {
    const row: Record<string, number | string | null> = { ts };
    for (const s of series) {
      const pt = s.points.find((p) => p.crawled_at === ts);
      row[s.product_name] = pt?.median_price ?? null;
    }
    return row;
  });

  const displayProdNames = series.map((s) => s.product_name);

  return (
    <div>
      <PageHeader
        title="价格趋势"
        description="按抓取批次聚合的中位数价格走势，支持多商品对比"
      />
      <Card>
        <Space direction="vertical" size={12} style={{ width: '100%' }}>
          <Space wrap>
            <Select
              mode="multiple"
              loading={productsLoading}
              placeholder="选择商品（可多选对比）"
              style={{ minWidth: 320 }}
              maxTagCount="responsive"
              options={allProducts.map((p) => ({ label: p.name, value: p.id }))}
              value={selectedIds}
              onChange={setSelectedIds}
              filterOption={(input, option) =>
                String(option?.label ?? '').toLowerCase().includes(input.toLowerCase())
              }
            />
            <Button type="primary" onClick={loadTrend} disabled={selectedIds.length === 0} loading={chartLoading}>
              查看趋势
            </Button>
            {series.length > 0 && (
              <Button
                type="text"
                onClick={() => {
                  setSeries([]);
                  setSelectedIds([]);
                }}
              >
                清除
              </Button>
            )}
          </Space>

          {series.length > 0 && (
            <Space size={8} wrap>
              {series.map((s, i) => (
                <Tag key={s.product_id} color={COLORS[i % COLORS.length]}>
                  {s.product_name}
                  {s.points.length > 0 && `（${s.points.length} 批次）`}
                </Tag>
              ))}
            </Space>
          )}

          {chartLoading ? (
            <div style={{ height: 320, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <Spin size="large" />
            </div>
          ) : series.length > 0 && chartData.length > 0 ? (
            <div style={{ height: 320 }}>
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData} margin={{ top: 5, right: 30, left: 20, bottom: 5 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke={token.colorSplit} />
                  <XAxis
                    dataKey="ts"
                    tickFormatter={(v: number) =>
                      new Date(v * 1000).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' })
                    }
                    tick={{ fill: token.colorTextSecondary, fontSize: 12 }}
                    axisLine={{ stroke: token.colorBorderSecondary }}
                    tickLine={{ stroke: token.colorBorderSecondary }}
                  />
                  <YAxis
                    tickFormatter={(v: number) => `¥${v.toFixed(0)}`}
                    tick={{ fill: token.colorTextSecondary, fontSize: 12 }}
                    axisLine={{ stroke: token.colorBorderSecondary }}
                    tickLine={{ stroke: token.colorBorderSecondary }}
                  />
                  <Tooltip
                    labelFormatter={(v: number) => new Date(v * 1000).toLocaleString('zh-CN', { hour12: false })}
                    formatter={(value: number) => [`¥${value.toFixed(2)}`, undefined]}
                    contentStyle={{
                      background: token.colorBgElevated,
                      border: `1px solid ${token.colorBorderSecondary}`,
                      borderRadius: token.borderRadius,
                    }}
                    labelStyle={{ color: token.colorText }}
                    itemStyle={{ color: token.colorTextSecondary }}
                    cursor={{ stroke: token.colorBorderSecondary }}
                  />
                  {displayProdNames.length > 1 && <Legend wrapperStyle={{ color: token.colorTextSecondary }} />}
                  {displayProdNames.map((name, i) => (
                    <Line
                      key={name}
                      type="monotone"
                      dataKey={name}
                      stroke={COLORS[i % COLORS.length]}
                      strokeWidth={2}
                      dot={{ r: 3 }}
                      connectNulls
                      name={name}
                    />
                  ))}
                </LineChart>
              </ResponsiveContainer>
            </div>
          ) : series.length > 0 ? (
            <Empty description="所选商品还未有抓取记录" style={{ padding: '48px 0' }} />
          ) : (
            <Empty
              description={
                <>
                  <div style={{ fontWeight: 500 }}>选择商品查看价格走势</div>
                  <div style={{ fontSize: 13 }}>
                    勾选多个商品可对比趋势。价格按抓取批次（同一时间戳为一轮）聚合，展示中位数。
                  </div>
                </>
              }
              style={{ padding: '48px 0' }}
            />
          )}
        </Space>
      </Card>
    </div>
  );
}
