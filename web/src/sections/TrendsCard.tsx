import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { ChartLine, TrendingUp } from 'lucide-react';
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
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Skeleton } from '@/components/ui/skeleton';
import { apiGet } from '@/lib/api';
import type { PageResponse, PriceTrendSeries, Product } from '@/types/api';

const COLORS = [
  '#3b82f6', '#ef4444', '#22c55e', '#f59e0b', '#8b5cf6',
  '#ec4899', '#06b6d4', '#f97316', '#6366f1', '#14b8a6',
];

export function TrendsCard() {
  const [allProducts, setAllProducts] = useState<Product[]>([]);
  const [productsLoading, setProductsLoading] = useState(true);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [series, setSeries] = useState<PriceTrendSeries[]>([]);
  const [chartLoading, setChartLoading] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const page = await apiGet<PageResponse<Product>>('/api/products?page=1&page_size=1000');
        setAllProducts(page.items);
      } catch {
        /* ignore */
      } finally {
        setProductsLoading(false);
      }
    })();
  }, []);

  const toggleProduct = (id: number) => {
    const next = new Set(selectedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelectedIds(next);
  };

  const loadTrend = async () => {
    const ids = [...selectedIds];
    if (ids.length === 0) {
      toast.error('请先选择商品');
      return;
    }
    setChartLoading(true);
    try {
      const data = await apiGet<PriceTrendSeries[]>(
        `/api/products/price-trend?product_ids=${ids.join(',')}`,
      );
      setSeries(data);
    } catch (e) {
      toast.error(`加载趋势数据失败: ${(e as Error).message}`);
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
    <Card>
      <CardHeader>
        <CardTitle>价格趋势</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
              <Button variant="outline" disabled={productsLoading}>
                选择商品 ({selectedIds.size})
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-64 max-h-80 overflow-y-auto" align="start">
              {productsLoading ? (
                <div className="space-y-1 p-2">
                  <Skeleton className="h-6 w-full" />
                  <Skeleton className="h-6 w-full" />
                  <Skeleton className="h-6 w-full" />
                </div>
              ) : allProducts.length === 0 ? (
                <p className="text-sm text-muted-foreground p-2">暂无商品</p>
              ) : (
                allProducts.map((p) => (
                  <label
                    key={p.id}
                    className="flex items-center gap-2 rounded-sm px-2 py-1.5 text-sm hover:bg-accent cursor-pointer"
                  >
                    <Checkbox
                      checked={selectedIds.has(p.id)}
                      onCheckedChange={() => toggleProduct(p.id)}
                    />
                    {p.name}
                  </label>
                ))
              )}
            </PopoverContent>
          </Popover>
          <Button onClick={loadTrend} disabled={selectedIds.size === 0 || chartLoading}>
            {chartLoading ? '加载中...' : '查看趋势'}
          </Button>
          {series.length > 0 && (
            <button
              className="text-sm text-muted-foreground hover:text-foreground"
              onClick={() => { setSeries([]); setSelectedIds(new Set()); }}
            >
              清除
            </button>
          )}
        </div>

        {series.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {series.map((s, i) => (
              <Badge key={s.product_id} variant="outline" style={{ borderColor: COLORS[i % COLORS.length] }}>
                <span
                  className="mr-1 inline-block h-2 w-2 rounded-full"
                  style={{ backgroundColor: COLORS[i % COLORS.length] }}
                />
                {s.product_name}
                {s.points.length > 0 && (
                  <span className="ml-1 text-xs text-muted-foreground">
                    ({s.points.length} 批次)
                  </span>
                )}
              </Badge>
            ))}
          </div>
        )}

        {chartLoading ? (
          <Skeleton className="h-80 w-full" />
        ) : series.length > 0 && chartData.length > 0 ? (
          <div className="h-80">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={chartData} margin={{ top: 5, right: 30, left: 20, bottom: 5 }}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="ts"
                  tickFormatter={(v: number) =>
                    new Date(v * 1000).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' })
                  }
                  className="text-xs"
                />
                <YAxis
                  tickFormatter={(v: number) => `¥${v.toFixed(0)}`}
                  className="text-xs"
                />
                <Tooltip
                  labelFormatter={(v: number) =>
                    new Date(v * 1000).toLocaleString('zh-CN', { hour12: false })
                  }
                  formatter={(value: number) => [`¥${value.toFixed(2)}`, undefined]}
                />
                {displayProdNames.length > 1 && <Legend />}
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
        ) : series.length > 0 && chartData.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <ChartLine />
              </EmptyMedia>
              <EmptyTitle>暂无趋势数据</EmptyTitle>
              <EmptyDescription>所选商品还未有抓取记录。</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <TrendingUp />
              </EmptyMedia>
              <EmptyTitle>选择商品查看价格走势</EmptyTitle>
              <EmptyDescription>
                勾选多个商品可对比趋势。价格按抓取批次（同一时间戳为一轮）聚合，展示中位数。
                {allProducts.length > 0 && ' 上方选择商品后点击「查看趋势」。'}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </CardContent>
    </Card>
  );
}
