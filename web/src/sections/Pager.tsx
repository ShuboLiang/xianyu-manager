import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

interface Props {
  page: number;
  pageSize: number;
  total: number;
  onChange: (page: number, pageSize: number) => void;
}

const PAGE_SIZES = [20, 50, 100];

/** 通用分页条：总数/页码 + 每页条数 + 上一页/下一页 */
export function Pager({ page, pageSize, total, onChange }: Props) {
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  return (
    <div className="flex flex-wrap items-center justify-between gap-2 pt-3">
      <span className="font-data text-xs text-muted-foreground">
        共 {total} 条 · 第 {page}/{totalPages} 页
      </span>
      <div className="flex items-center gap-2">
        <Select value={String(pageSize)} onValueChange={(v) => onChange(1, Number(v))}>
          <SelectTrigger className="h-8 w-28">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PAGE_SIZES.map((n) => (
              <SelectItem key={n} value={String(n)}>
                {n} 条/页
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          variant="outline"
          size="sm"
          disabled={page <= 1}
          onClick={() => onChange(page - 1, pageSize)}
        >
          上一页
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={page >= totalPages}
          onClick={() => onChange(page + 1, pageSize)}
        >
          下一页
        </Button>
      </div>
    </div>
  );
}
