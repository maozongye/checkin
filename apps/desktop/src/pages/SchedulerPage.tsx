import { useEffect, useState, type ReactNode } from "react";
import { PageHeader } from "@/components/layout/PageHeader";
import { schedulerGet, schedulerSet } from "@/lib/api";
import type { SchedulerConfig } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

export function SchedulerPage() {
  const [config, setConfig] = useState<SchedulerConfig | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void schedulerGet().then(setConfig);
  }, []);

  async function toggle() {
    if (!config) return;
    setSaving(true);
    try {
      const next = await schedulerSet({ ...config, enabled: !config.enabled });
      setConfig(next);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div>
      <PageHeader
        title="定时"
        description="App 内调度；v1 需托盘常驻。系统任务计划留 v1.1。"
        actions={
          <Button variant="secondary" size="sm" disabled={saving || !config} onClick={() => void toggle()}>
            {config?.enabled ? "关闭定时" : "开启定时"}
          </Button>
        }
      />

      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="状态">
          {config ? (
            <Badge variant={config.enabled ? "success" : "default"}>
              {config.enabled ? "已开启" : "已关闭"}
            </Badge>
          ) : (
            "…"
          )}
        </Field>
        <Field label="每日时间">{config?.cronOrTime ?? "—"}</Field>
        <Field label="时区">{config?.timezone ?? "—"}</Field>
        <Field label="平台过滤">
          {config?.platformsFilter?.join(", ") || "全部"}
        </Field>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="rounded-lg border border-border bg-bg-elevated p-4">
      <div className="text-xs text-fg-subtle">{label}</div>
      <div className="mt-2 text-sm text-fg">{children}</div>
    </div>
  );
}
