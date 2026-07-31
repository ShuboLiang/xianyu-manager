import { Inbox } from 'lucide-react';
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { fmtTime } from '@/lib/api';
import { SkeletonRows } from '@/sections/SkeletonRows';
import type { Item } from '@/types/api';

interface Props {
  items: Item[];
  loading?: boolean;
  onRefresh: () => void;
}

export function ItemsCard({ items, loading, onRefresh }: Props) {
  return (
    <Card>
      <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-2 space-y-0">
        <CardTitle>商品列表（已抓取的原始数据）</CardTitle>
        <Button onClick={onRefresh}>刷新列表</Button>
      </CardHeader>
      <CardContent>
        {loading ? (
          <Table>
            <TableBody>
              <SkeletonRows cols={5} rows={4} />
            </TableBody>
          </Table>
        ) : items.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Inbox />
              </EmptyMedia>
              <EmptyTitle>暂无抓取数据</EmptyTitle>
              <EmptyDescription>
                队列执行后，抓到的原始数据会出现在这里。可以点右上角「刷新列表」手动拉取。
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button variant="outline" size="sm" onClick={onRefresh}>
                刷新列表
              </Button>
            </EmptyContent>
          </Empty>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
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
                  <TableCell>{it.title}</TableCell>
                  <TableCell className="font-data">¥{it.price}</TableCell>
                  <TableCell>{it.seller}</TableCell>
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
        )}
      </CardContent>
    </Card>
  );
}
