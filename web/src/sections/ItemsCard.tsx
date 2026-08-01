import { useEffect, useRef, useState } from 'react';
import { Inbox, Search, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty';
import { Input } from '@/components/ui/input';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { fmtTime } from '@/lib/api';
import { Pager } from '@/sections/Pager';
import { SkeletonRows } from '@/sections/SkeletonRows';
import type { Item } from '@/types/api';

interface Props {
  items: Item[];
  total: number;
  page: number;
  pageSize: number;
  search: string;
  loading?: boolean;
  onPageChange: (page: number, pageSize: number) => void;
  onSearchChange: (search: string) => void;
  onRefresh: () => void;
}

export function ItemsCard({
  items,
  total,
  page,
  pageSize,
  search,
  loading,
  onPageChange,
  onSearchChange,
  onRefresh,
}: Props) {
  const [localSearch, setLocalSearch] = useState(search);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    setLocalSearch(search);
  }, [search]);

  const handleSearchChange = (value: string) => {
    setLocalSearch(value);
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => {
      onSearchChange(value.trim());
    }, 300);
  };

  return (
    <Card>
      <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-2 space-y-0">
        <CardTitle>商品列表（已抓取的原始数据）</CardTitle>
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="w-44 pl-8"
              placeholder="搜索标题/商品名..."
              value={localSearch}
              onChange={(e) => handleSearchChange(e.target.value)}
            />
            {localSearch && (
              <button
                className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                onClick={() => handleSearchChange('')}
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
          <Button onClick={onRefresh}>刷新列表</Button>
        </div>
      </CardHeader>
      <CardContent>
        {loading ? (
          <Table>
            <TableBody>
              <SkeletonRows cols={6} rows={4} />
            </TableBody>
          </Table>
        ) : items.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Inbox />
              </EmptyMedia>
              <EmptyTitle>{search ? '无匹配结果' : '暂无抓取数据'}</EmptyTitle>
              <EmptyDescription>
                {search
                  ? '未找到匹配的商品名或标题，请尝试其他关键词。'
                  : '队列执行后，抓到的原始数据会出现在这里。可以点右上角「刷新列表」手动拉取。'}
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button variant="outline" size="sm" onClick={onRefresh}>
                刷新列表
              </Button>
            </EmptyContent>
          </Empty>
        ) : (
          <div className="space-y-3">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>商品名</TableHead>
                  <TableHead>标题</TableHead>
                  <TableHead>价格</TableHead>
                  <TableHead>卖家</TableHead>
                  <TableHead>抓取时间</TableHead>
                  <TableHead>链接</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((it) => (
                  <TableRow key={it.id}>
                    <TableCell className="max-w-32">
                      <span className="block truncate" title={it.product_name ?? undefined}>
                        {it.product_name || '-'}
                      </span>
                    </TableCell>
                    <TableCell className="max-w-64">
                      <span className="block truncate" title={it.title}>
                        {it.title}
                      </span>
                    </TableCell>
                    <TableCell className="whitespace-nowrap font-data">¥{it.price}</TableCell>
                    <TableCell className="max-w-32">
                      <span className="block truncate" title={it.seller}>
                        {it.seller}
                      </span>
                    </TableCell>
                    <TableCell className="whitespace-nowrap">{fmtTime(it.crawled_at)}</TableCell>
                    <TableCell>
                      <a className="text-primary hover:underline" href={it.url} target="_blank" rel="noreferrer">
                        查看
                      </a>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <Pager page={page} pageSize={pageSize} total={total} onChange={onPageChange} />
          </div>
        )}
      </CardContent>
    </Card>
  );
}
