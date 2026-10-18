import { useState } from "react";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import {
  getAllowLiveHttp,
  setAllowLiveHttp,
} from "@/lib/checkinSettings";
import {
  DEFAULT_HEALTH_INTERVAL_MINUTES,
  getHealthIntervalMinutes,
  setHealthIntervalMinutes,
} from "@/lib/monitorSettings";

export function SettingsPage() {
  const [intervalMin, setIntervalMin] = useState(() => getHealthIntervalMinutes());
  const [savedHint, setSavedHint] = useState<string | null>(null);
  const [allowLiveHttp, setAllowLiveHttpState] = useState(() => getAllowLiveHttp());

  function onSaveInterval() {
    const n = setHealthIntervalMinutes(intervalMin);
    setIntervalMin(n);
    setSavedHint(`已保存：${n} 分钟`);
  }

  function onToggleLiveHttp(next: boolean) {
    const saved = setAllowLiveHttp(next);
    setAllowLiveHttpState(saved);
  }

  return (
    <div>
      <PageHeader title="设置" description="外观、签到行为、数据与危险操作" />
      <div className="space-y-3">
        {[
          ["主题", "仅深色（v1）"],
          ["字体", "系统字体栈"],
          ["窗口", "系统原生标题栏"],
          ["签到取消", "v1 不支持取消进行中任务"],
          ["签到并发", "默认 3（可配）"],
          ["数据目录", "由 Tauri 本地路径决定"],
          ["导出日志", "脱敏，不含 token"],
        ].map(([k, v]) => (
          <div
            key={k}
            className="flex items-center justify-between rounded-lg border border-border bg-bg-elevated px-4 py-3 text-sm"
          >
            <span className="text-fg">{k}</span>
            <span className="text-fg-muted">{v}</span>
          </div>
        ))}

        <div className="rounded-lg border border-border bg-bg-elevated px-4 py-3 text-sm">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0 flex-1">
              <div className="text-fg">允许真实签到 HTTP</div>
              <p className="mt-0.5 text-xs text-fg-subtle">
                绑定 <code className="text-fg-muted">allow_live_http</code>
                ，默认关闭。关闭时 <code className="text-fg-muted">checkin_run_all</code>{" "}
                仅 dry-run；开启才 live。
              </p>
              <p className="mt-1 text-xs text-fg-muted">
                当前：{allowLiveHttp ? "已开启 · live" : "已关闭 · 仅 dry-run"}
              </p>
            </div>
            <label className="inline-flex cursor-pointer items-center gap-2 shrink-0">
              <span className="text-xs text-fg-muted">{allowLiveHttp ? "开" : "关"}</span>
              <input
                type="checkbox"
                className="h-4 w-4 accent-fg"
                checked={allowLiveHttp}
                onChange={(e) => onToggleLiveHttp(e.target.checked)}
              />
            </label>
          </div>
        </div>

        <div className="rounded-lg border border-border bg-bg-elevated px-4 py-3 text-sm">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <div className="text-fg">探活间隔</div>
              <p className="mt-0.5 text-xs text-fg-subtle">
                壳层按此间隔调用 monitor_refresh（默认{" "}
                {DEFAULT_HEALTH_INTERVAL_MINUTES} 分钟；范围 5–1440）
              </p>
            </div>
            <div className="flex items-center gap-2">
              <input
                type="number"
                min={5}
                max={1440}
                className="w-24 rounded-md border border-border bg-bg px-2 py-1.5 text-sm tabular-nums text-fg"
                value={intervalMin}
                onChange={(e) => setIntervalMin(Number(e.target.value))}
              />
              <span className="text-xs text-fg-muted">分钟</span>
              <Button size="sm" variant="secondary" onClick={onSaveInterval}>
                保存
              </Button>
            </div>
          </div>
          {savedHint ? (
            <p className="mt-2 text-xs text-success">{savedHint}</p>
          ) : null}
        </div>

        <div className="rounded-lg border border-danger/40 bg-danger/5 px-4 py-3 text-sm text-danger">
          Danger Zone（占位）：清空本地缓存 / 重置调度 — 需二次确认
        </div>
      </div>
    </div>
  );
}
