import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { ArrowDown, ArrowUp, ArrowUpDown, Package } from 'lucide-react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty';
import { Input } from '@/components/ui/input';
import { Progress } from '@/components/ui/progress';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Textarea } from '@/components/ui/textarea';
import { apiDelete, apiGet, apiPost, apiPut, fmtPrice, fmtTime } from '@/lib/api';
import { Pager } from '@/sections/Pager';
import { SkeletonRows } from '@/sections/SkeletonRows';
import type {
  ClassifyProductsResponse,
  ClassifyTask,
  Item,
  Product,
  ProductBatchCreateResponse,
  Tag,
} from '@/types/api';

interface Props {
  products: Product[]; // 当前页数据（服务端分页 + 排序）
  total: number;
  page: number;
  pageSize: number;
  sortBy: string | null;
  sortDir: 'asc' | 'desc';
  tags: Tag[];
  loading?: boolean;
  onPageChange: (page: number, pageSize: number) => void;
  onSortChange: (sortBy: SortKey) => void;
  onRefresh: () => void;
  onRefreshAiCalls: () => void;
  onEnqueueProducts: (ids: number[]) => Promise<boolean>;
}

const TERMINAL_TASK_STATUS = ['done', 'failed', 'cancelled'];

// 可排序的数值/时间列：服务端排序，空值永远排在最后
type SortKey = 'median_price' | 'avg_price' | 'crawled_count' | 'last_crawled_at' | 'recycle_price';

export function ProductsCard({
  products,
  total,
  page,
  pageSize,
  sortBy,
  sortDir,
  tags,
  loading,
  onPageChange,
  onSortChange,
  onRefresh,
  onRefreshAiCalls,
  onEnqueueProducts,
}: Props) {
  const [editingId, setEditingId] = useState<number | null>(null);
  const [name, setName] = useState('');
  const [remark, setRemark] = useState('');
  const [formTagIds, setFormTagIds] = useState<Set<number>>(new Set());
  const [checkedIds, setCheckedIds] = useState<Set<number>>(new Set());
  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);

  // 抓取明细弹窗：detailProduct 非空即打开；detailItems=null 表示加载中
  const [detailProduct, setDetailProduct] = useState<Product | null>(null);
  const [detailItems, setDetailItems] = useState<Item[] | null>(null);

  const openDetail = async (p: Product) => {
    setDetailProduct(p);
    setDetailItems(null);
    try {
      setDetailItems(await apiGet<Item[]>(`/api/products/${p.id}/latest-items`));
    } catch (e) {
      toast.error(`加载抓取明细失败: ${(e as Error).message}`);
      setDetailProduct(null);
    }
  };

  const SortHead = ({ label, k }: { label: string; k: SortKey }) => (
    <TableHead>
      <button
        className="flex items-center gap-1 whitespace-nowrap hover:text-foreground"
        onClick={() => onSortChange(k)}
      >
        {label}
        {sortBy === k ? (
          sortDir === 'asc' ? (
            <ArrowUp className="h-3 w-3" />
          ) : (
            <ArrowDown className="h-3 w-3" />
          )
        ) : (
          <ArrowUpDown className="h-3 w-3 opacity-40" />
        )}
      </button>
    </TableHead>
  );

  // 批量导入
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchText, setBatchText] = useState('');
  const [batchTagIds, setBatchTagIds] = useState<Set<number>>(new Set());
  const [batchResult, setBatchResult] = useState<string | null>(null);
  const [batchSubmitting, setBatchSubmitting] = useState(false);

  // AI 自动打标签
  const [classifying, setClassifying] = useState(false);
  const [classifyTask, setClassifyTask] = useState<ClassifyTask | null>(null);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => stopPolling(), []);

  const stopPolling = () => {
    if (pollTimer.current) {
      clearTimeout(pollTimer.current);
      pollTimer.current = null;
    }
  };

  const toggleFormTag = (id: number, on: boolean) => {
    const next = new Set(formTagIds);
    if (on) next.add(id);
    else next.delete(id);
    setFormTagIds(next);
  };

  const toggleChecked = (id: number, on: boolean) => {
    const next = new Set(checkedIds);
    if (on) next.add(id);
    else next.delete(id);
    setCheckedIds(next);
  };

  const resetForm = () => {
    setEditingId(null);
    setName('');
    setRemark('');
    setFormTagIds(new Set());
  };

  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      toast.error('商品名不能为空');
      return;
    }
    const isEdit = editingId !== null;
    const payload = { name: trimmed, tag_ids: [...formTagIds], remark: remark.trim() || null };
    try {
      if (isEdit) {
        await apiPut(`/api/products/${editingId}`, payload);
      } else {
        await apiPost('/api/products', payload);
      }
      resetForm();
      onRefresh();
    } catch (e) {
      toast.error(`${isEdit ? '更新' : '创建'}失败: ${(e as Error).message}`);
    }
  };

  const startEdit = async (id: number) => {
    try {
      const p = await apiGet<Product>(`/api/products/${id}`);
      setEditingId(p.id);
      setName(p.name);
      setRemark(p.remark || '');
      setFormTagIds(new Set(p.tag_ids));
    } catch (e) {
      toast.error(`加载商品失败: ${(e as Error).message}`);
    }
  };

  const confirmDelete = async () => {
    if (pendingDeleteId === null) return;
    try {
      await apiDelete(`/api/products/${pendingDeleteId}`);
      setPendingDeleteId(null);
      onRefresh();
    } catch (e) {
      toast.error(`删除失败: ${(e as Error).message}`);
    }
  };

  const crawlOne = async (id: number) => {
    await onEnqueueProducts([id]);
  };

  const crawlSelected = async () => {
    const ids = [...checkedIds];
    if (ids.length === 0) {
      toast.error('请先勾选商品');
      return;
    }
    const ok = await onEnqueueProducts(ids);
    if (ok) setCheckedIds(new Set());
  };

  // ---------- 批量导入 ----------

  const submitBatch = async () => {
    const names = batchText
      .split(/\n/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (names.length === 0) {
      toast.error('请输入至少一个商品名');
      return;
    }
    if (names.length > 1000) {
      toast.error(`最多 1000 条，当前 ${names.length} 条`);
      return;
    }
    setBatchSubmitting(true);
    try {
      const tag_ids = [...batchTagIds];
      const data = await apiPost<ProductBatchCreateResponse>('/api/products/batch', {
        names,
        tag_ids: tag_ids.length ? tag_ids : null,
      });
      let html = `创建 ${data.created.length} 条`;
      if (data.created.length) html += '：' + data.created.map((p) => p.name).join('、');
      if (data.skipped.length) {
        html += `；跳过 ${data.skipped.length} 条：` +
          data.skipped.map((s) => `${s.name}（${s.reason}）`).join('、');
      }
      setBatchResult(html);
      if (data.created.length > 0) {
        setBatchText('');
        onRefresh();
      }
    } catch (e) {
      toast.error(`导入失败: ${(e as Error).message}`);
    } finally {
      setBatchSubmitting(false);
    }
  };

  // ---------- AI 自动打标签 ----------

  const aiClassify = async () => {
    const ids = [...checkedIds];
    if (ids.length === 0) {
      toast.error('请先勾选商品');
      return;
    }
    if (ids.length <= 50) {
      setClassifying(true);
      try {
        const data = await apiPost<ClassifyProductsResponse>('/api/ai/classify-products', {
          product_ids: ids,
        });
        let msg = `AI 已完成分类，涉及 ${data.suggestions.length} 个商品`;
        if (data.warnings.length) msg += `，有 ${data.warnings.length} 条警告：\n${data.warnings.join('\n')}`;
        toast.success(msg);
        onRefresh();
        onRefreshAiCalls();
      } catch (e) {
        toast.error(`AI 分类失败: ${(e as Error).message}`);
      } finally {
        setClassifying(false);
      }
    } else {
      try {
        const task = await apiPost<ClassifyTask>('/api/ai/classify-tasks', { product_ids: ids });
        setClassifyTask(task);
        pollClassify(task.id);
      } catch (e) {
        toast.error(`创建分类任务失败: ${(e as Error).message}`);
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
          onRefresh();
          onRefreshAiCalls();
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
      onRefresh();
    } catch (e) {
      toast.error(`取消失败: ${(e as Error).message}`);
    }
  };

  const classifyPct =
    classifyTask && classifyTask.total > 0
      ? Math.round((classifyTask.processed / classifyTask.total) * 100)
      : 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle>待爬取商品管理</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-2">
          <Input
            className="w-56"
            placeholder="商品名，如：佳能 5D4 机身"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Input
            className="w-56"
            placeholder="备注（可选）"
            value={remark}
            onChange={(e) => setRemark(e.target.value)}
          />
          <Button onClick={submit}>{editingId !== null ? '保存修改' : '添加商品'}</Button>
          {editingId !== null && (
            <Button variant="secondary" onClick={resetForm}>
              取消编辑
            </Button>
          )}
          <Button variant="secondary" onClick={() => { setBatchResult(null); setBatchOpen(true); }}>
            批量导入
          </Button>
          <Button variant="secondary" disabled={classifying} onClick={aiClassify}>
            {classifying ? 'AI 分类中...' : 'AI 自动打标签'}
          </Button>
          <Button variant="secondary" onClick={crawlSelected}>
            选中加入队列
          </Button>
        </div>

        {classifyTask && (
          <div className="flex items-center gap-3 rounded-md border bg-muted/50 px-3 py-2">
            <Progress value={classifyPct} className="flex-1" />
            <span className="text-sm whitespace-nowrap">
              已处理 {classifyTask.processed}/{classifyTask.total}
              {classifyTask.failed > 0 && `，失败 ${classifyTask.failed}`} | 状态: {classifyTask.status}
              {classifyTask.error && ` | 错误: ${classifyTask.error}`}
            </span>
            {!TERMINAL_TASK_STATUS.includes(classifyTask.status) && (
              <Button variant="secondary" size="sm" onClick={cancelClassify}>
                取消
              </Button>
            )}
          </div>
        )}

        <div className="flex flex-wrap gap-x-4 gap-y-2">
          {tags.length === 0 ? (
            <span className="text-sm text-muted-foreground">暂无标签可勾选</span>
          ) : (
            tags.map((t) => (
              <label key={t.id} className="flex items-center gap-1.5 text-sm">
                <Checkbox checked={formTagIds.has(t.id)} onCheckedChange={(v) => toggleFormTag(t.id, v === true)} />
                {t.name}
              </label>
            ))
          )}
        </div>

        {loading ? (
          <Table>
            <TableBody>
              <SkeletonRows cols={10} rows={4} />
            </TableBody>
          </Table>
        ) : products.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Package />
              </EmptyMedia>
              <EmptyTitle>暂无商品</EmptyTitle>
              <EmptyDescription>
                在上方输入商品名添加，或用「批量导入」一次创建多个，再加入队列开始抓取。
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className="space-y-3">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-8"></TableHead>
                  <TableHead>商品名</TableHead>
                  <TableHead>标签</TableHead>
                  <SortHead label="中位数" k="median_price" />
                  <SortHead label="均价" k="avg_price" />
                  <SortHead label="爬取数量" k="crawled_count" />
                  <SortHead label="最后爬取" k="last_crawled_at" />
                  <SortHead label="回收价" k="recycle_price" />
                  <TableHead>备注</TableHead>
                  <TableHead>操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
              {products.map((p) => (
                <TableRow key={p.id}>
                  <TableCell>
                    <Checkbox
                      checked={checkedIds.has(p.id)}
                      onCheckedChange={(v) => toggleChecked(p.id, v === true)}
                    />
                  </TableCell>
                  <TableCell>{p.name}</TableCell>
                  <TableCell>
                    {p.tag_names.length ? (
                      p.tag_names.join('、')
                    ) : (
                      <span className="text-muted-foreground">无标签</span>
                    )}
                  </TableCell>
                  <TableCell className="font-data">{fmtPrice(p.median_price)}</TableCell>
                  <TableCell className="font-data">{fmtPrice(p.avg_price)}</TableCell>
                  <TableCell className="font-data">{p.crawled_count ?? '-'}</TableCell>
                  <TableCell className="whitespace-nowrap font-data text-xs">
                    {fmtTime(p.last_crawled_at)}
                  </TableCell>
                  <TableCell className="font-data">{fmtPrice(p.recycle_price)}</TableCell>
                  <TableCell>{p.remark || '-'}</TableCell>
                  <TableCell className="space-x-3 whitespace-nowrap">
                    <button className="text-primary hover:underline" onClick={() => crawlOne(p.id)}>
                      抓取
                    </button>
                    <button className="text-primary hover:underline" onClick={() => openDetail(p)}>
                      明细
                    </button>
                    <button className="text-primary hover:underline" onClick={() => startEdit(p.id)}>
                      编辑
                    </button>
                    <button className="text-destructive hover:underline" onClick={() => setPendingDeleteId(p.id)}>
                      删除
                    </button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
            </Table>
            <Pager page={page} pageSize={pageSize} total={total} onChange={onPageChange} />
          </div>
        )}

        <Dialog open={batchOpen} onOpenChange={setBatchOpen}>
          <DialogContent className="max-w-lg">
            <DialogHeader>
              <DialogTitle>批量导入商品</DialogTitle>
            </DialogHeader>
            <p className="text-sm text-muted-foreground">每行一个商品名，最多 1000 条</p>
            <Textarea
              rows={10}
              placeholder={'佳能 5D4\n索尼 A7M3 机身\n富士 X-T5 套机'}
              value={batchText}
              onChange={(e) => setBatchText(e.target.value)}
            />
            <div>
              <p className="mb-2 text-sm text-muted-foreground">统一标签（可选）：</p>
              <div className="flex flex-wrap gap-x-4 gap-y-2">
                {tags.length === 0 ? (
                  <span className="text-sm text-muted-foreground">暂无标签</span>
                ) : (
                  tags.map((t) => (
                    <label key={t.id} className="flex items-center gap-1.5 text-sm">
                      <Checkbox
                        checked={batchTagIds.has(t.id)}
                        onCheckedChange={(v) => {
                          const next = new Set(batchTagIds);
                          if (v === true) next.add(t.id);
                          else next.delete(t.id);
                          setBatchTagIds(next);
                        }}
                      />
                      {t.name}
                    </label>
                  ))
                )}
              </div>
            </div>
            {batchResult && (
              <div className="rounded-md border bg-muted/50 px-3 py-2 text-sm">{batchResult}</div>
            )}
            <div className="flex gap-2">
              <Button disabled={batchSubmitting} onClick={submitBatch}>
                {batchSubmitting ? '提交中...' : '提交导入'}
              </Button>
              <Button variant="secondary" onClick={() => setBatchOpen(false)}>
                关闭
              </Button>
            </div>
          </DialogContent>
        </Dialog>

        <Dialog open={detailProduct !== null} onOpenChange={(o) => !o && setDetailProduct(null)}>
          <DialogContent className="max-w-3xl">
            <DialogHeader>
              <DialogTitle>
                「{detailProduct?.name}」最新一轮抓取明细
                {detailItems && <span className="ml-2 text-sm font-normal text-muted-foreground">共 {detailItems.length} 条</span>}
              </DialogTitle>
            </DialogHeader>
            {detailItems === null ? (
              <Table>
                <TableBody>
                  <SkeletonRows cols={4} rows={4} />
                </TableBody>
              </Table>
            ) : detailItems.length === 0 ? (
              <p className="py-6 text-center text-sm text-muted-foreground">
                暂无抓取明细——该商品还没有完成过一轮抓取，点「抓取」开始。
              </p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>标题</TableHead>
                    <TableHead className="w-24">价格</TableHead>
                    <TableHead className="w-28">卖家</TableHead>
                    <TableHead className="w-14">链接</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {detailItems.map((it) => (
                    <TableRow key={it.id}>
                      <TableCell className="max-w-md">
                        <span className="line-clamp-2">{it.title}</span>
                      </TableCell>
                      <TableCell className="font-data">¥{it.price}</TableCell>
                      <TableCell>{it.seller || '-'}</TableCell>
                      <TableCell>
                        <a className="text-primary hover:underline" href={it.url} target="_blank" rel="noreferrer">
                          查看
                        </a>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </DialogContent>
        </Dialog>

        <AlertDialog open={pendingDeleteId !== null} onOpenChange={(o) => !o && setPendingDeleteId(null)}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                删除商品「{products.find((p) => p.id === pendingDeleteId)?.name}」？
              </AlertDialogTitle>
              <AlertDialogDescription>
                删除后其标签关联与抓取统计将一并清除；若该商品在活跃队列中，对应条目会被跳过。此操作不可恢复。
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>取消</AlertDialogCancel>
              <AlertDialogAction
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                onClick={confirmDelete}
              >
                确认删除
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </CardContent>
    </Card>
  );
}
