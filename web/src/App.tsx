import { useCallback, useEffect, useRef, useState } from 'react';
import { useTheme } from 'next-themes';
import { HashRouter, NavLink, Navigate, Route, Routes } from 'react-router';
import {
  AlertTriangle,
  Bot,
  ChartLine,
  Inbox,
  LayoutDashboard,
  Menu,
  Moon,
  Package,
  Sun,
  Tags as TagsIcon,
  type LucideIcon,
} from 'lucide-react';
import { Toaster } from '@/components/ui/sonner';
import { toast } from 'sonner';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Sheet, SheetContent, SheetTrigger } from '@/components/ui/sheet';
import { apiGet, apiPost } from '@/lib/api';
import { cn } from '@/lib/utils';
import { AiCard } from '@/sections/AiCard';
import { ItemsCard } from '@/sections/ItemsCard';
import { KpiStrip } from '@/sections/KpiStrip';
import { ProductsCard } from '@/sections/ProductsCard';
import { QueuesCard } from '@/sections/QueuesCard';
import { TagsCard } from '@/sections/TagsCard';
import { TrendsCard } from '@/sections/TrendsCard';
import type {
  AiProvider,
  AiStatus,
  AiToolCall,
  CrawlPrompt,
  EnqueueResponse,
  Item,
  PageResponse,
  Product,
  QueueProgress,
  Selector,
  StatsResponse,
  Tag,
} from '@/types/api';

// 分页查询条件（App 层持有，卡片只是视图）
interface PageQuery {
  page: number;
  pageSize: number;
}

interface ProductsQuery extends PageQuery {
  sortBy: string | null;
  sortDir: 'asc' | 'desc';
  search: string;
}

// 侧边导航：概览（监控）/ 商品 / 标签 / 数据 / AI（低频配置）
const NAV_ITEMS: { to: string; label: string; icon: LucideIcon; end?: boolean }[] = [
  { to: '/', label: '概览', icon: LayoutDashboard, end: true },
  { to: '/products', label: '商品管理', icon: Package },
  { to: '/tags', label: '标签管理', icon: TagsIcon },
  { to: '/items', label: '抓取数据', icon: Inbox },
  { to: '/trends', label: '价格趋势', icon: ChartLine },
  { to: '/ai', label: 'AI', icon: Bot },
];

export default function App() {
  const { resolvedTheme, setTheme } = useTheme();
  const [healthy, setHealthy] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);
  const [tags, setTags] = useState<Tag[]>([]);
  const [queues, setQueues] = useState<QueueProgress[]>([]);
  const [aiProviders, setAiProviders] = useState<AiProvider[]>([]);
  const [aiStatus, setAiStatus] = useState<AiStatus | null>(null);
  const [crawlPrompt, setCrawlPrompt] = useState('');
  const [stats, setStats] = useState<StatsResponse | null>(null);

  // 分页列表：查询条件 + 当前页数据（商品/原始数据/AI 调用记录）
  const [productsQuery, setProductsQuery] = useState<ProductsQuery>({
    page: 1,
    pageSize: 20,
    sortBy: null,
    sortDir: 'desc',
    search: '',
  });
  const [productsPage, setProductsPage] = useState<PageResponse<Product>>({
    items: [],
    total: 0,
    page: 1,
    page_size: 20,
  });
  const [itemsQuery, setItemsQuery] = useState<PageQuery>({ page: 1, pageSize: 20 });
  const [itemsPage, setItemsPage] = useState<PageResponse<Item>>({
    items: [],
    total: 0,
    page: 1,
    page_size: 20,
  });
  const [aiCallsQuery, setAiCallsQuery] = useState<PageQuery>({ page: 1, pageSize: 20 });
  const [aiCallsPage, setAiCallsPage] = useState<PageResponse<AiToolCall>>({
    items: [],
    total: 0,
    page: 1,
    page_size: 20,
  });

  const [appendTarget, setAppendTarget] = useState<number | null>(null);
  const [intervalSecs, setIntervalSecs] = useState(3);

  const queueWasActive = useRef(false);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ---------- 数据加载 ----------

  const loadTags = useCallback(async () => {
    try {
      setTags(await apiGet<Tag[]>('/api/tags'));
    } catch {
      /* 首次后端未就绪时静默 */
    }
  }, []);

  const loadProducts = useCallback(
    async (q: ProductsQuery = productsQuery) => {
      const params = new URLSearchParams({ page: String(q.page), page_size: String(q.pageSize) });
      if (q.sortBy) {
        params.set('sort_by', q.sortBy);
        params.set('sort_dir', q.sortDir);
      }
      if (q.search) {
        params.set('search', q.search);
      }
      try {
        setProductsPage(await apiGet<PageResponse<Product>>(`/api/products?${params}`));
      } catch {
        /* ignore */
      }
    },
    [productsQuery],
  );

  const loadItems = useCallback(
    async (q: PageQuery = itemsQuery) => {
      try {
        setItemsPage(await apiGet<PageResponse<Item>>(`/api/items?page=${q.page}&page_size=${q.pageSize}`));
      } catch {
        /* ignore */
      }
    },
    [itemsQuery],
  );

  const loadAiCalls = useCallback(
    async (q: PageQuery = aiCallsQuery) => {
      try {
        setAiCallsPage(
          await apiGet<PageResponse<AiToolCall>>(`/api/ai/tool-calls?page=${q.page}&page_size=${q.pageSize}`),
        );
      } catch {
        /* ignore */
      }
    },
    [aiCallsQuery],
  );

  const loadStats = useCallback(async () => {
    try {
      setStats(await apiGet<StatsResponse>('/api/stats'));
    } catch {
      /* ignore */
    }
  }, []);

  // 分页/排序变更：更新查询条件并按新条件重新拉取（排序变更回到第 1 页）
  const changeProductsPage = useCallback(
    (page: number, pageSize: number) => {
      const q = { ...productsQuery, page, pageSize };
      setProductsQuery(q);
      loadProducts(q);
    },
    [productsQuery, loadProducts],
  );

  const changeProductsSort = useCallback(
    (sortBy: string) => {
      const sortDir =
        productsQuery.sortBy === sortBy && productsQuery.sortDir === 'desc' ? 'asc' : 'desc';
      const q: ProductsQuery = { ...productsQuery, page: 1, sortBy, sortDir };
      setProductsQuery(q);
      loadProducts(q);
    },
    [productsQuery, loadProducts],
  );

  const changeProductsSearch = useCallback(
    (search: string) => {
      const q: ProductsQuery = { ...productsQuery, page: 1, search };
      setProductsQuery(q);
      loadProducts(q);
    },
    [productsQuery, loadProducts],
  );

  const changeItemsPage = useCallback(
    (page: number, pageSize: number) => {
      const q = { page, pageSize };
      setItemsQuery(q);
      loadItems(q);
    },
    [loadItems],
  );

  const changeAiCallsPage = useCallback(
    (page: number, pageSize: number) => {
      const q = { page, pageSize };
      setAiCallsQuery(q);
      loadAiCalls(q);
    },
    [loadAiCalls],
  );

  const loadQueues = useCallback(async () => {
    try {
      const list = await apiGet<QueueProgress[]>('/api/queues');
      setQueues(list);
      const active = list.some((q) => ['waiting', 'running'].includes(q.status));
      if (pollTimer.current) {
        clearTimeout(pollTimer.current);
        pollTimer.current = null;
      }
      if (active) {
        queueWasActive.current = true;
        pollTimer.current = setTimeout(loadQueues, 2000);
      } else if (queueWasActive.current) {
        // 刚全部结束：最后刷新一次商品统计、原始数据和 KPI
        queueWasActive.current = false;
        loadProducts();
        loadItems();
        loadStats();
      }
    } catch {
      /* ignore */
    }
  }, [loadProducts, loadItems, loadStats]);

  const loadAi = useCallback(async () => {
    try {
      const [providers, status, prompt] = await Promise.all([
        apiGet<AiProvider[]>('/api/ai/providers'),
        apiGet<AiStatus>('/api/ai/status'),
        apiGet<CrawlPrompt>('/api/ai/crawl-prompt'),
      ]);
      setAiProviders(providers);
      setAiStatus(status);
      setCrawlPrompt(prompt.custom_prompt);
    } catch {
      /* ignore */
    }
  }, []);

  const checkHealth = useCallback(async () => {
    try {
      await apiGet<string>('/api/health');
      setHealthy(true);
    } catch {
      setHealthy(false);
    }
  }, []);

  // 首次加载：所有数据并行拉取，期间各区块显示骨架屏
  const loadAll = useCallback(async () => {
    await Promise.allSettled([
      checkHealth(),
      loadTags(),
      loadProducts(),
      loadQueues(),
      loadItems(),
      loadAi(),
      loadAiCalls(),
      loadStats(),
    ]);
    setLoading(false);
  }, [checkHealth, loadTags, loadProducts, loadQueues, loadItems, loadAi, loadAiCalls, loadStats]);

  useEffect(() => {
    loadAll();
    return () => {
      if (pollTimer.current) clearTimeout(pollTimer.current);
    };
  }, [loadAll]);

  const onTagsChanged = useCallback(() => {
    loadTags().then(() => loadProducts());
  }, [loadTags, loadProducts]);

  // ---------- 入队（选择器 / 商品 id 两种目标，追加模式共用） ----------

  const enqueue = useCallback(
    async (target: { selector: Selector } | { product_ids: number[] }): Promise<boolean> => {
      const isAppend = appendTarget !== null;
      const url = isAppend ? `/api/queues/${appendTarget}/entries` : '/api/queues';
      try {
        const data = await apiPost<EnqueueResponse>(url, { ...target, interval_secs: intervalSecs });
        let msg = `${isAppend ? '追加' : '入队'}成功：新增 ${data.added.length} 个`;
        if (data.skipped.length) msg += `，跳过 ${data.skipped.length} 个（已在队列）`;
        if (!isAppend && data.status === 'waiting') msg += '；已有队列在执行，本队列进入排队，将自动开始';
        toast.success(msg);
        if (isAppend) setAppendTarget(null);
        loadQueues();
        return true;
      } catch (e) {
        toast.error(`${isAppend ? '追加' : '入队'}失败: ${(e as Error).message}`);
        return false;
      }
    },
    [appendTarget, intervalSecs, loadQueues],
  );

  const enqueueSelector = useCallback((selector: Selector) => enqueue({ selector }), [enqueue]);
  const enqueueProducts = useCallback((ids: number[]) => enqueue({ product_ids: ids }), [enqueue]);

  const enterAppend = useCallback((id: number) => {
    setAppendTarget(id);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }, []);

  const [sheetOpen, setSheetOpen] = useState(false);
  const queueRunning = queues.some((q) => q.status === 'running');

  const healthBadge =
    healthy === null ? (
      <Badge variant="secondary">检测服务中…</Badge>
    ) : healthy ? (
      <Badge>服务正常</Badge>
    ) : (
      <Badge variant="destructive">服务不可用</Badge>
    );

  const themeToggle = (iconOnly: boolean) => (
    <Button
      variant="ghost"
      size={iconOnly ? 'icon' : 'sm'}
      className={iconOnly ? '' : 'w-full justify-start gap-2'}
      title={resolvedTheme === 'dark' ? '切换为浅色模式' : '切换为深色模式'}
      onClick={() => setTheme(resolvedTheme === 'dark' ? 'light' : 'dark')}
    >
      {resolvedTheme === 'dark' ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
      {!iconOnly && (resolvedTheme === 'dark' ? '浅色模式' : '深色模式')}
    </Button>
  );

  // 侧边导航列表；队列运行时「概览」项尾部显示呼吸点，任何页面都能感知系统在跑
  const renderNav = (onNavigate?: () => void) =>
    NAV_ITEMS.map((item) => (
      <NavLink
        key={item.to}
        to={item.to}
        end={item.end}
        onClick={onNavigate}
        className={({ isActive }) =>
          cn(
            'flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors',
            isActive
              ? 'bg-accent font-medium text-accent-foreground'
              : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
          )
        }
      >
        <item.icon className="h-4 w-4" />
        {item.label}
        {item.to === '/' && queueRunning && (
          <span className="ml-auto h-2 w-2 animate-pulse rounded-full bg-primary" title="有队列正在执行" />
        )}
      </NavLink>
    ));

  return (
    <HashRouter>
      <div className="min-h-screen bg-muted/30 md:flex">
        {/* 桌面端：左侧固定导航 */}
        <aside className="sticky top-0 hidden h-screen w-56 shrink-0 flex-col border-r bg-background md:flex">
          <div className="px-4 py-4">
            <h1 className="text-lg font-semibold">闲鱼管理台</h1>
            <p className="mt-0.5 text-xs text-muted-foreground">二手行情监控</p>
          </div>
          <nav className="flex-1 space-y-1 px-2">{renderNav()}</nav>
          <div className="space-y-2 border-t p-3">
            <div>{healthBadge}</div>
            {themeToggle(false)}
          </div>
        </aside>

        <div className="min-w-0 flex-1">
          {/* 移动端：顶栏 + 抽屉菜单 */}
          <header className="sticky top-0 z-10 flex items-center gap-3 border-b bg-background/95 px-4 py-3 backdrop-blur md:hidden">
            <Sheet open={sheetOpen} onOpenChange={setSheetOpen}>
              <SheetTrigger asChild>
                <Button variant="ghost" size="icon" title="打开导航菜单">
                  <Menu className="h-5 w-5" />
                </Button>
              </SheetTrigger>
              <SheetContent side="left" className="w-64 p-0">
                <div className="border-b px-4 py-4">
                  <h1 className="text-lg font-semibold">闲鱼管理台</h1>
                  <p className="mt-0.5 text-xs text-muted-foreground">二手行情监控</p>
                </div>
                <nav className="space-y-1 p-2">{renderNav(() => setSheetOpen(false))}</nav>
              </SheetContent>
            </Sheet>
            <h1 className="text-base font-semibold">闲鱼管理台</h1>
            <span className="ml-auto flex items-center gap-1">
              {healthBadge}
              {themeToggle(true)}
            </span>
          </header>

          <main className="mx-auto max-w-6xl space-y-6 px-4 py-6">
            {healthy === false && (
              <Alert variant="destructive">
                <AlertTriangle />
                <AlertTitle>后端服务不可用</AlertTitle>
                <AlertDescription>
                  <p>无法连接后端接口，请确认服务已启动（cargo run，默认 http://127.0.0.1:3000）。</p>
                  <Button size="sm" variant="outline" onClick={loadAll}>
                    重试连接
                  </Button>
                </AlertDescription>
              </Alert>
            )}
            <Routes>
              <Route
                path="/"
                element={
                  <div className="space-y-6">
                    <KpiStrip queues={queues} stats={stats} loading={loading} />
                    <QueuesCard
                      tags={tags}
                      queues={queues}
                      loading={loading}
                      appendTarget={appendTarget}
                      intervalSecs={intervalSecs}
                      onIntervalChange={setIntervalSecs}
                      onExitAppend={() => setAppendTarget(null)}
                      onEnterAppend={enterAppend}
                      onRefresh={loadQueues}
                      onEnqueueSelector={enqueueSelector}
                    />
                  </div>
                }
              />
              <Route
                path="/products"
                element={
                  <ProductsCard
                    products={productsPage.items}
                    total={productsPage.total}
                    page={productsQuery.page}
                    pageSize={productsQuery.pageSize}
                    sortBy={productsQuery.sortBy}
                    sortDir={productsQuery.sortDir}
                    search={productsQuery.search}
                    tags={tags}
                    loading={loading}
                    onPageChange={changeProductsPage}
                    onSortChange={changeProductsSort}
                    onSearchChange={changeProductsSearch}
                    onRefresh={loadProducts}
                    onRefreshAiCalls={loadAiCalls}
                    onEnqueueProducts={enqueueProducts}
                  />
                }
              />
              <Route path="/tags" element={<TagsCard tags={tags} loading={loading} onChanged={onTagsChanged} />} />
              <Route
                path="/items"
                element={
                  <ItemsCard
                    items={itemsPage.items}
                    total={itemsPage.total}
                    page={itemsQuery.page}
                    pageSize={itemsQuery.pageSize}
                    loading={loading}
                    onPageChange={changeItemsPage}
                    onRefresh={loadItems}
                  />
                }
              />
              <Route path="/trends" element={<TrendsCard />} />
              <Route
                path="/ai"
                element={
                  <AiCard
                    providers={aiProviders}
                    status={aiStatus}
                    crawlPrompt={crawlPrompt}
                    toolCalls={aiCallsPage}
                    onCallsPageChange={changeAiCallsPage}
                    onRefresh={loadAi}
                  />
                }
              />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </main>
        </div>
        <Toaster richColors position="top-center" theme={resolvedTheme === 'dark' ? 'dark' : 'light'} />
      </div>
    </HashRouter>
  );
}
