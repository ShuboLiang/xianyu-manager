import { useState } from 'react';
import {
  App as AntApp,
  Button,
  Card,
  DatePicker,
  Form,
  Input,
  InputNumber,
  Modal,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import { DeleteOutlined, EditOutlined, PlayCircleOutlined, PlusOutlined } from '@ant-design/icons';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query';
import dayjs, { type Dayjs } from 'dayjs';
import { PageHeader } from '@/components/PageHeader';
import { apiDelete, apiGet, apiPost, apiPut, fmtTime } from '@/lib/api';
import { useTags } from '@/lib/queries';
import type { Schedule, Tag as TagType } from '@/types/api';

type ScheduleForm = {
  name: string;
  tag_ids: number[];
  every_days: number;
  queue_interval_secs: number;
  next_run_at: Dayjs;
};

export function SchedulesPage() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();
  const { data: tags = [] } = useTags();
  const { data: schedules = [], isLoading } = useQuery({
    queryKey: ['schedules'],
    queryFn: () => apiGet<Schedule[]>('/api/schedules'),
    placeholderData: keepPreviousData,
  });
  const [form] = Form.useForm<ScheduleForm>();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Schedule | null>(null);

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['schedules'] });
    queryClient.invalidateQueries({ queryKey: ['queues'] });
  };

  const openCreate = () => {
    setEditing(null);
    form.setFieldsValue({
      name: '',
      tag_ids: [],
      every_days: 7,
      queue_interval_secs: 3,
      next_run_at: dayjs().add(5, 'minute').second(0),
    });
    setOpen(true);
  };

  const openEdit = (schedule: Schedule) => {
    setEditing(schedule);
    form.setFieldsValue({
      name: schedule.name,
      tag_ids: schedule.tag_ids,
      every_days: schedule.every_days,
      queue_interval_secs: schedule.queue_interval_secs,
      next_run_at: dayjs(schedule.next_run_at * 1000),
    });
    setOpen(true);
  };

  const submit = async () => {
    const values = await form.validateFields();
    const payload = {
      name: values.name.trim(),
      tag_ids: values.tag_ids,
      every_days: values.every_days,
      queue_interval_secs: values.queue_interval_secs,
      ...(editing
        ? { next_run_at: Math.floor(values.next_run_at.valueOf() / 1000) }
        : { first_run_at: Math.floor(values.next_run_at.valueOf() / 1000) }),
    };
    try {
      if (editing) {
        await apiPut(`/api/schedules/${editing.id}`, payload);
      } else {
        await apiPost('/api/schedules', payload);
      }
      message.success(editing ? '定时任务已保存' : '定时任务已创建');
      setOpen(false);
      refresh();
    } catch (e) {
      message.error(`保存失败: ${(e as Error).message}`);
    }
  };

  const toggle = async (schedule: Schedule) => {
    try {
      await apiPut(`/api/schedules/${schedule.id}`, { enabled: !schedule.enabled });
      message.success(schedule.enabled ? '任务已暂停' : '任务已启用');
      refresh();
    } catch (e) {
      message.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const runNow = (schedule: Schedule) => {
    modal.confirm({
      title: `立即执行「${schedule.name}」？`,
      content: '将按该任务当前标签筛选商品并创建普通抓取队列，不会改变原来的下次执行时间。',
      okText: '立即执行',
      cancelText: '取消',
      onOk: async () => {
        try {
          const updated = await apiPost<Schedule>(`/api/schedules/${schedule.id}/run`);
          message.success(updated.last_message || '已执行');
          refresh();
        } catch (e) {
          message.error(`执行失败: ${(e as Error).message}`);
        }
      },
    });
  };

  const remove = (schedule: Schedule) => {
    modal.confirm({
      title: `删除定时任务「${schedule.name}」？`,
      content: '已创建的抓取队列不会受影响。',
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await apiDelete(`/api/schedules/${schedule.id}`);
          message.success('已删除定时任务');
          refresh();
        } catch (e) {
          message.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  const tagName = (id: number) => tags.find((tag) => tag.id === id)?.name ?? `标签 #${id}`;
  const columns = [
    {
      title: '任务',
      dataIndex: 'name',
      width: 180,
      render: (name: string, row: Schedule) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong ellipsis={{ tooltip: name }} style={{ maxWidth: 160 }}>
            {name}
          </Typography.Text>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            商品间隔 {row.queue_interval_secs} 秒
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: '抓取范围',
      dataIndex: 'tag_ids',
      width: 150,
      render: (ids: number[]) => ids.map((id) => <Tag key={id}>{tagName(id)}</Tag>),
    },
    { title: '周期', dataIndex: 'every_days', width: 105, render: (days: number) => `每 ${days} 天` },
    {
      title: '下次执行',
      dataIndex: 'next_run_at',
      width: 175,
      render: (v: number, row: Schedule) =>
        row.active_queue_id ? (
          <Space direction="vertical" size={0}>
            <Tag color="processing" style={{ margin: 0 }}>本轮进行中</Tag>
            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
              等待队列结束后计时
            </Typography.Text>
          </Space>
        ) : (
          fmtTime(v)
        ),
    },
    {
      title: '最近结果',
      dataIndex: 'last_message',
      width: 260,
      render: (messageText: string | null, row: Schedule) => (
        <Tooltip title={messageText || undefined}>
          <Space direction="vertical" size={0} style={{ maxWidth: 245 }}>
            <Typography.Text ellipsis style={{ maxWidth: 245 }}>
              {messageText || '-'}
            </Typography.Text>
            {row.last_run_at && (
              <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                {fmtTime(row.last_run_at)}
              </Typography.Text>
            )}
          </Space>
        </Tooltip>
      ),
    },
    {
      title: '状态',
      dataIndex: 'enabled',
      width: 88,
      render: (enabled: boolean, row: Schedule) => (
        <Switch size="small" checked={enabled} checkedChildren="启用" unCheckedChildren="暂停" onChange={() => void toggle(row)} />
      ),
    },
    {
      title: '操作',
      width: 152,
      render: (_: unknown, row: Schedule) => (
        <Space size={0}>
          <Tooltip title="立即执行">
            <Button type="link" size="small" icon={<PlayCircleOutlined />} disabled={!row.enabled} onClick={() => runNow(row)} />
          </Tooltip>
          <Tooltip title="编辑">
            <Button type="link" size="small" icon={<EditOutlined />} onClick={() => openEdit(row)} />
          </Tooltip>
          <Tooltip title="删除">
            <Button danger type="link" size="small" icon={<DeleteOutlined />} onClick={() => remove(row)} />
          </Tooltip>
        </Space>
      ),
    },
  ];

  return (
    <div className="schedules-page">
      <PageHeader
        title="定时任务"
        description="任务到点后会自动创建抓取队列，实际抓取仍由系统串行执行。"
        extra={<Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>新建定时任务</Button>}
      />
      <Card className="schedules-card">
        <Table<Schedule>
          rowKey="id"
          size="middle"
          loading={isLoading}
          columns={columns}
          dataSource={schedules}
          pagination={false}
          scroll={{ x: 1080 }}
          locale={{ emptyText: '还没有定时任务，创建一条例如「显卡每 7 天抓取一次」。' }}
        />
      </Card>

      <Modal
        title={editing ? '编辑定时任务' : '新建定时任务'}
        open={open}
        onCancel={() => setOpen(false)}
        onOk={() => void submit()}
        okText="保存"
        cancelText="取消"
        destroyOnHidden
      >
        <Form form={form} layout="vertical" preserve={false}>
          <Form.Item name="name" label="任务名称" rules={[{ required: true, whitespace: true, message: '请输入任务名称' }]}>
            <Input placeholder="例如：显卡每周巡检" maxLength={64} />
          </Form.Item>
          <Form.Item name="tag_ids" label="抓取标签" rules={[{ required: true, type: 'array', min: 1, message: '至少选择一个标签' }]}>
            <Select
              mode="multiple"
              placeholder="选择此任务要抓取的商品标签"
              options={tags.filter((t: TagType) => t.enabled).map((t: TagType) => ({ value: t.id, label: t.name }))}
            />
          </Form.Item>
          <Space size="middle" style={{ display: 'flex' }} align="start">
            <Form.Item name="every_days" label="执行周期" rules={[{ required: true }]}>
              <InputNumber min={1} max={365} addonAfter="天" style={{ width: 130 }} />
            </Form.Item>
            <Form.Item name="queue_interval_secs" label="商品间隔" rules={[{ required: true }]}>
              <InputNumber min={1} max={3600} addonAfter="秒" style={{ width: 140 }} />
            </Form.Item>
          </Space>
          <Form.Item name="next_run_at" label={editing ? '下次执行时间' : '首次执行时间'} rules={[{ required: true, message: '请选择执行时间' }]}>
            <DatePicker showTime format="YYYY-MM-DD HH:mm" style={{ width: '100%' }} />
          </Form.Item>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            到点后会重新按所选标签筛选商品；队列结束后才开始计算下一周期，不会并发或积压。
          </Typography.Text>
        </Form>
      </Modal>
    </div>
  );
}
