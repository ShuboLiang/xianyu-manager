import { useState } from 'react';
import { toast } from 'sonner';
import { Tags as TagsIcon } from 'lucide-react';
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
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Empty,
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
import { apiDelete, apiGet, apiPost, apiPut } from '@/lib/api';
import { SkeletonRows } from '@/sections/SkeletonRows';
import type { ProductBrief, Tag } from '@/types/api';

interface Props {
  tags: Tag[];
  loading?: boolean;
  onChanged: () => void; // 标签变化后刷新标签与商品
}

export function TagsCard({ tags, loading, onChanged }: Props) {
  const [editingId, setEditingId] = useState<number | null>(null);
  const [name, setName] = useState('');
  const [remark, setRemark] = useState('');
  // 待确认删除的标签及其使用商品（在 AlertDialog 中展示影响范围）
  const [pendingDelete, setPendingDelete] = useState<{ tag: Tag; used: ProductBrief[] } | null>(null);

  const resetForm = () => {
    setEditingId(null);
    setName('');
    setRemark('');
  };

  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      toast.error('标签名不能为空');
      return;
    }
    const isEdit = editingId !== null;
    try {
      if (isEdit) {
        await apiPut(`/api/tags/${editingId}`, { name: trimmed, remark: remark.trim() || null });
      } else {
        await apiPost('/api/tags', { name: trimmed, remark: remark.trim() || null });
      }
      resetForm();
      onChanged();
    } catch (e) {
      toast.error(`${isEdit ? '更新' : '创建'}失败: ${(e as Error).message}`);
    }
  };

  const startEdit = async (id: number) => {
    try {
      const t = await apiGet<Tag>(`/api/tags/${id}`);
      setEditingId(t.id);
      setName(t.name);
      setRemark(t.remark || '');
    } catch (e) {
      toast.error(`加载标签失败: ${(e as Error).message}`);
    }
  };

  const toggle = async (tag: Tag) => {
    try {
      await apiPut(`/api/tags/${tag.id}`, { enabled: !tag.enabled });
      onChanged();
    } catch (e) {
      toast.error(`切换状态失败: ${(e as Error).message}`);
    }
  };

  const remove = async (tag: Tag) => {
    // 删除前查询该标签正被哪些商品使用，在确认框中列出影响范围
    try {
      const used = await apiGet<ProductBrief[]>(`/api/tags/${tag.id}/products`);
      setPendingDelete({ tag, used });
    } catch (e) {
      toast.error(`查询标签使用情况失败: ${(e as Error).message}`);
    }
  };

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const { tag } = pendingDelete;
    try {
      await apiDelete(`/api/tags/${tag.id}`);
      if (editingId === tag.id) resetForm();
      setPendingDelete(null);
      onChanged();
    } catch (e) {
      toast.error(`删除失败: ${(e as Error).message}`);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>标签管理</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-2">
          <Input
            className="w-56"
            placeholder="标签名，如：二手相机"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Input
            className="w-56"
            placeholder="备注（可选）"
            value={remark}
            onChange={(e) => setRemark(e.target.value)}
          />
          <Button onClick={submit}>{editingId !== null ? '保存修改' : '添加标签'}</Button>
          {editingId !== null && (
            <Button variant="secondary" onClick={resetForm}>
              取消编辑
            </Button>
          )}
        </div>
        {loading ? (
          <Table>
            <TableBody>
              <SkeletonRows cols={4} />
            </TableBody>
          </Table>
        ) : tags.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <TagsIcon />
              </EmptyMedia>
              <EmptyTitle>暂无标签</EmptyTitle>
              <EmptyDescription>标签决定爬虫抓取哪一类商品，先在上方创建一个。</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>标签名</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>备注</TableHead>
                <TableHead>操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {tags.map((t) => (
                <TableRow key={t.id}>
                  <TableCell>{t.name}</TableCell>
                  <TableCell>
                    <Badge variant={t.enabled ? 'default' : 'secondary'}>
                      {t.enabled ? '启用' : '停用'}
                    </Badge>
                  </TableCell>
                  <TableCell>{t.remark || '-'}</TableCell>
                  <TableCell className="space-x-3">
                    <button className="text-primary hover:underline" onClick={() => startEdit(t.id)}>
                      编辑
                    </button>
                    <button className="text-primary hover:underline" onClick={() => toggle(t)}>
                      {t.enabled ? '停用' : '启用'}
                    </button>
                    <button
                      className="text-destructive hover:underline"
                      onClick={() => remove(t)}
                    >
                      删除
                    </button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>

      <AlertDialog open={pendingDelete !== null} onOpenChange={(o) => !o && setPendingDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除标签「{pendingDelete?.tag.name}」？</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-2">
                {pendingDelete && pendingDelete.used.length > 0 ? (
                  <>
                    <p>该标签正被 {pendingDelete.used.length} 个商品使用，删除后这些商品将移除此标签：</p>
                    <ul className="max-h-40 space-y-1 overflow-y-auto rounded-md border bg-muted/50 px-3 py-2">
                      {pendingDelete.used.map((p) => (
                        <li key={p.id}>· {p.name}</li>
                      ))}
                    </ul>
                  </>
                ) : (
                  <p>该标签未被任何商品使用。删除后不可恢复。</p>
                )}
              </div>
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
    </Card>
  );
}
