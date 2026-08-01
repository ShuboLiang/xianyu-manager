import { useState } from 'react';
import {
  App as AntApp,
  Button,
  Card,
  Form,
  Input,
  Modal,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { PageHeader } from '@/components/PageHeader';
import { apiDelete, apiGet, apiPost, apiPut } from '@/lib/api';
import { useTags } from '@/lib/queries';
import type { ProductBrief, Tag as TagType } from '@/types/api';

const TAG_COLORS = ['blue', 'green', 'orange', 'purple', 'cyan', 'magenta', 'geekblue', 'volcano'];

export function TagsPage() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();
  const { data: tags = [], isLoading } = useTags();

  const [formOpen, setFormOpen] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form] = Form.useForm<{ name: string; remark?: string }>();

  const onChanged = () => {
    queryClient.invalidateQueries({ queryKey: ['tags'] });
    queryClient.invalidateQueries({ queryKey: ['products'] });
  };

  const openCreate = () => {
    setEditingId(null);
    form.resetFields();
    setFormOpen(true);
  };

  const openEdit = async (id: number) => {
    try {
      const t = await apiGet<TagType>(`/api/tags/${id}`);
      setEditingId(t.id);
      form.setFieldsValue({ name: t.name, remark: t.remark ?? undefined });
      setFormOpen(true);
    } catch (e) {
      message.error(`加载标签失败: ${(e as Error).message}`);
    }
  };

  const submitForm = async () => {
    const values = await form.validateFields();
    const payload = { name: values.name.trim(), remark: values.remark?.trim() || null };
    const isEdit = editingId !== null;
    try {
      if (isEdit) {
        await apiPut(`/api/tags/${editingId}`, payload);
      } else {
        await apiPost('/api/tags', payload);
      }
      message.success(isEdit ? '已保存修改' : '已添加标签');
      setFormOpen(false);
      onChanged();
    } catch (e) {
      message.error(`${isEdit ? '更新' : '创建'}失败: ${(e as Error).message}`);
    }
  };

  const toggle = async (tag: TagType) => {
    try {
      await apiPut(`/api/tags/${tag.id}`, { enabled: !tag.enabled });
      onChanged();
    } catch (e) {
      message.error(`切换状态失败: ${(e as Error).message}`);
    }
  };

  const remove = async (tag: TagType) => {
    // 删除前查询该标签正被哪些商品使用，在确认框中列出影响范围
    let used: ProductBrief[] = [];
    try {
      used = await apiGet<ProductBrief[]>(`/api/tags/${tag.id}/products`);
    } catch (e) {
      message.error(`查询标签使用情况失败: ${(e as Error).message}`);
      return;
    }
    modal.confirm({
      title: `删除标签「${tag.name}」？`,
      content:
        used.length > 0 ? (
          <div>
            <p>该标签正被 {used.length} 个商品使用，删除后这些商品将移除此标签：</p>
            <ul style={{ maxHeight: 160, overflowY: 'auto', paddingLeft: 18, margin: 0 }}>
              {used.map((p) => (
                <li key={p.id}>{p.name}</li>
              ))}
            </ul>
          </div>
        ) : (
          '该标签未被任何商品使用。删除后不可恢复。'
        ),
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await apiDelete(`/api/tags/${tag.id}`);
          message.success('已删除');
          onChanged();
        } catch (e) {
          message.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  return (
    <div>
      <PageHeader
        title="标签管理"
        description="标签决定爬虫抓取哪一类商品；停用的标签不参与抓取"
        extra={
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            新建标签
          </Button>
        }
      />
      <Card>
        <Table<TagType>
          rowKey="id"
          loading={isLoading}
          dataSource={tags}
          pagination={false}
          locale={{ emptyText: '暂无标签，点右上角「新建标签」创建一个' }}
          columns={[
            {
              title: '标签名',
              dataIndex: 'name',
              width: 220,
              render: (v: string, t) => <Tag color={TAG_COLORS[t.id % TAG_COLORS.length]}>{v}</Tag>,
            },
            {
              title: '状态',
              dataIndex: 'enabled',
              width: 140,
              render: (v: boolean, t) => (
                <Space size={8}>
                  <Switch size="small" checked={v} onChange={() => toggle(t)} />
                  <Typography.Text type={v ? undefined : 'secondary'} style={{ fontSize: 13 }}>
                    {v ? '启用' : '停用'}
                  </Typography.Text>
                </Space>
              ),
            },
            {
              title: '备注',
              dataIndex: 'remark',
              ellipsis: true,
              render: (v: string | null) => v || <Typography.Text type="secondary">-</Typography.Text>,
            },
            {
              title: '操作',
              key: 'actions',
              width: 130,
              render: (_, t) => (
                <Space split={<Typography.Text type="secondary">|</Typography.Text>} size={2}>
                  <Button type="link" size="small" style={{ padding: 0 }} onClick={() => openEdit(t.id)}>
                    编辑
                  </Button>
                  <Button type="link" size="small" danger style={{ padding: 0 }} onClick={() => remove(t)}>
                    删除
                  </Button>
                </Space>
              ),
            },
          ]}
        />
      </Card>

      <Modal
        title={editingId !== null ? '编辑标签' : '新建标签'}
        open={formOpen}
        onOk={submitForm}
        onCancel={() => setFormOpen(false)}
        okText={editingId !== null ? '保存修改' : '添加标签'}
        cancelText="取消"
        destroyOnHidden
      >
        <Form form={form} layout="vertical">
          <Form.Item
            name="name"
            label="标签名"
            rules={[{ required: true, whitespace: true, message: '标签名不能为空' }]}
          >
            <Input placeholder="如：二手相机" />
          </Form.Item>
          <Form.Item name="remark" label="备注">
            <Input.TextArea rows={2} placeholder="可选" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
