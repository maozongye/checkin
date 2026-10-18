import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { RefreshCw, X } from "lucide-react";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { EVENTS, listen, monitorGetDashboard, monitorRefresh } from "@/lib/api";
import type {
  MonitorAccountRow,
  MonitorAlert,
  MonitorDashboard,
  SessionHealthStatus,
} from "@/lib/types";
import { cn } from "@/lib/utils";


function alertDedupeKey(a: MonitorAlert): string {
  return a.dedupeKey ?? a.dedupe_key ?? a.id ?? `${a.rule}:${a.accountId ?? a.account_id ?? ""}`;
}

function alertTone(a: MonitorAlert): "error" | "warn" | "info" {
  const s = (a.severity ?? a.level ?? "info").toLowerCase();
  if (s === "error" || s === "critical" || s === "high") return "error";
  if (s === "warn" || s === "warning" || s === "medium") return "warn";
  return "info";
}

function alertText(a: MonitorAlert): string {
  return a.title || a.body || a.message || "";
}

function alertKey(a: MonitorAlert): string {
  return alertDedupeKey(a);
}

type Tab = "list" | "trend";

function formatTs(ms?: number | null): string {
  if (ms == null) return "—";
  try {
    return new Date(ms).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" });
  } catch {
    return "—";
  }
}

function sessionDotClass(status: SessionHealthStatus): string {
  switch (status) {
    case "ok":
      return "bg-success";
    case "expiring":
      return "bg-warning";
    case "expired":
      return "bg-danger";
    case "unknown":
    default:
      return "bg-fg-subtle";
  }
}

function sessionLabel(status: SessionHealthStatus): string {
  switch (status) {
    case "ok":
      return "正常";
    case "expiring":
      return "将过期";
    case "expired":
      return "需重登";
    case "unknown":
    default:
      return "未知";
  }
}

function providerLabel(p: MonitorAccountRow["provider"]): string {
  return p === "workbuddy" ? "WorkBuddy" : "TraeCode CN";
}

export function MonitorPage() {
  const [dash, setDash] = useState<MonitorDashboard | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("list");
  const [selected, setSelected] = useState<MonitorAccountRow | null>(null);
  const [toastAlerts, setToastAlerts] = useState<MonitorAlert[]>([]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const d = await monitorGetDashboard();
      setDash(d);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      const d = await monitorRefresh();
      setDash(d);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void listen<MonitorAlert>(EVENTS.monitorAlert, (alert) => {
      if (!alive) return;
      // 预留 rule / severity / dedupe_key；同 key 替换而非堆叠
      setToastAlerts((prev) => {
        const key = alertDedupeKey(alert);
        const next = prev.filter((x) => alertDedupeKey(x) !== key);
        return [alert, ...next].slice(0, 5);
      });
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  return (
    <div>
      <PageHeader
        title="监控"
        description="积分、连签与会话健康。v1 不做消耗趋势。"
        actions={
          <Button
            variant="secondary"
            size="sm"
            disabled={refreshing || loading}
            onClick={() => void refresh()}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", refreshing && "animate-spin")} />
            刷新
          </Button>
        }
      />

      {/* 顶栏 summary */}
      <div className="mb-4 grid gap-3 sm:grid-cols-4">
        <SummaryCard
          label="健康"
          value={dash?.summary.healthy}
          tone="success"
          loading={loading}
        />
        <SummaryCard label="告警" value={dash?.summary.warn} tone="warning" loading={loading} />
        <SummaryCard label="异常" value={dash?.summary.error} tone="danger" loading={loading} />
        <div className="rounded-lg border border-border bg-bg-elevated p-4">
          <div className="text-xs text-fg-subtle">更新时间</div>
          <div className="mt-1 text-sm text-fg">
            {dash ? formatTs(dash.updatedAt) : loading ? "…" : "—"}
          </div>
          <div className="mt-1 text-[11px] text-fg-subtle">Asia/Shanghai</div>
        </div>
      </div>

      {toastAlerts.length > 0 ? (
        <div className="mb-4 space-y-1">
          {toastAlerts.map((a) => {
            const tone = alertTone(a);
            const key = alertKey(a);
            return (
            <div
              key={key}
              className={cn(
                "flex items-center justify-between rounded-md border px-3 py-2 text-xs",
                tone === "error"
                  ? "border-danger/40 bg-danger/10 text-danger"
                  : tone === "warn"
                    ? "border-warning/40 bg-warning/10 text-warning"
                    : "border-border bg-bg-elevated text-fg-muted",
              )}
            >
              <span className="min-w-0">
                {a.rule ? (
                  <span className="mr-2 font-mono text-[10px] opacity-70">{a.rule}</span>
                ) : null}
                {alertText(a)}
              </span>
              <button
                type="button"
                className="ml-2 shrink-0 text-fg-subtle hover:text-fg"
                onClick={() =>
                  setToastAlerts((p) => p.filter((x) => alertDedupeKey(x) !== key))
                }
                aria-label="关闭"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
            );
          })}
        </div>
      ) : null}

      {error ? (
        <div className="mb-4 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
          {error}
        </div>
      ) : null}

      {/* Tabs */}
      <div className="mb-3 flex gap-1 border-b border-border">
        <TabBtn active={tab === "list"} onClick={() => setTab("list")}>
          账号列表
        </TabBtn>
        <TabBtn active={tab === "trend"} onClick={() => setTab("trend")}>
          趋势
        </TabBtn>
      </div>

      {tab === "trend" ? (
        <div className="rounded-lg border border-dashed border-border p-10 text-center text-sm text-fg-subtle">
          消耗趋势（v1 不做）— 预留 Tab 占位
        </div>
      ) : (
        <div className="overflow-hidden rounded-lg border border-border">
          <table className="w-full text-left text-sm">
            <thead className="bg-bg-elevated text-xs text-fg-subtle">
              <tr>
                <th className="px-4 py-2 font-medium">账号</th>
                <th className="px-4 py-2 font-medium">平台</th>
                <th className="px-4 py-2 font-medium">积分</th>
                <th className="px-4 py-2 font-medium">连签</th>
                <th className="px-4 py-2 font-medium">会话</th>
                <th className="px-4 py-2 font-medium">最近签到</th>
              </tr>
            </thead>
            <tbody>
              {loading && !dash ? (
                <tr>
                  <td colSpan={6} className="px-4 py-8 text-center text-fg-muted">
                    加载中…
                  </td>
                </tr>
              ) : !dash?.accounts.length ? (
                <tr>
                  <td colSpan={6} className="px-4 py-8 text-center text-fg-muted">
                    暂无监控数据
                  </td>
                </tr>
              ) : (
                dash.accounts.map((row) => (
                  <tr
                    key={row.accountId}
                    className="cursor-pointer border-t border-border hover:bg-bg-hover/60"
                    onClick={() => setSelected(row)}
                  >
                    <td className="px-4 py-3 font-medium text-fg">{row.displayName}</td>
                    <td className="px-4 py-3 text-fg-muted">{providerLabel(row.provider)}</td>
                    <td className="px-4 py-3 tabular-nums text-fg">{row.pointsBalance}</td>
                    <td className="px-4 py-3 tabular-nums text-fg-muted">{row.streakDays}</td>
                    <td className="px-4 py-3">
                      <span className="inline-flex items-center gap-2 text-fg-muted">
                        <span
                          className={cn(
                            "inline-block h-2 w-2 rounded-full",
                            sessionDotClass(row.session.status),
                          )}
                          title={sessionLabel(row.session.status)}
                        />
                        {sessionLabel(row.session.status)}
                      </span>
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex items-center gap-2">
                        <span className="text-fg-muted">
                          {row.lastCheckin.status === "none"
                            ? "—"
                            : row.lastCheckin.status}
                        </span>
                        {row.lastCheckin.status === "fail" ||
                        row.lastCheckin.status === "failed" ? (
                          <Badge variant="danger">fail</Badge>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      )}

      {selected ? (
        <DetailDrawer row={selected} onClose={() => setSelected(null)} />
      ) : null}
    </div>
  );
}

function SummaryCard({
  label,
  value,
  tone,
  loading,
}: {
  label: string;
  value?: number;
  tone: "success" | "warning" | "danger";
  loading: boolean;
}) {
  const color =
    tone === "success" ? "text-success" : tone === "warning" ? "text-warning" : "text-danger";
  return (
    <div className="rounded-lg border border-border bg-bg-elevated p-4">
      <div className="text-xs text-fg-subtle">{label}</div>
      <div className={cn("mt-1 text-2xl font-semibold tabular-nums", color)}>
        {loading && value == null ? "…" : (value ?? "—")}
      </div>
    </div>
  );
}

function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "-mb-px border-b-2 px-3 py-2 text-sm transition-colors",
        active
          ? "border-accent text-fg"
          : "border-transparent text-fg-muted hover:text-fg",
      )}
    >
      {children}
    </button>
  );
}

function DetailDrawer({
  row,
  onClose,
}: {
  row: MonitorAccountRow;
  onClose: () => void;
}) {
  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button
        type="button"
        className="absolute inset-0 bg-black/50"
        aria-label="关闭抽屉"
        onClick={onClose}
      />
      <aside className="relative z-50 flex h-full w-full max-w-md flex-col border-l border-border bg-bg-elevated shadow-xl">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <div className="font-medium text-fg">{row.displayName}</div>
            <div className="text-xs text-fg-subtle">{providerLabel(row.provider)}</div>
          </div>
          <Button variant="ghost" size="icon" onClick={onClose} aria-label="关闭">
            <X className="h-4 w-4" />
          </Button>
        </div>
        <div className="flex-1 space-y-4 overflow-auto p-4 text-sm">
          <Section title="积分">
            <KV k="余额" v={String(row.pointsBalance)} />
            <KV k="连签天数" v={String(row.streakDays)} />
            <KV k="积分过期" v={formatTs(row.pointsExpireAt)} />
          </Section>
          <Section title="会话">
            <KV
              k="状态"
              v={
                <span className="inline-flex items-center gap-2">
                  <span
                    className={cn(
                      "inline-block h-2 w-2 rounded-full",
                      sessionDotClass(row.session.status),
                    )}
                  />
                  {sessionLabel(row.session.status)}
                </span>
              }
            />
            <KV k="会话过期" v={formatTs(row.session.expiresAt)} />
            <KV k="上次检查" v={formatTs(row.session.lastCheckedAt)} />
          </Section>
          <Section title="最近签到">
            <KV k="状态" v={row.lastCheckin.status} />
            <KV k="时间" v={formatTs(row.lastCheckin.at)} />
            <KV k="积分变动" v={row.lastCheckin.pointsDelta != null ? String(row.lastCheckin.pointsDelta) : "—"} />
            <KV k="详情" v={row.lastCheckin.message || "—"} />
          </Section>
        </div>
        <div className="flex items-center gap-4 border-t border-border px-4 py-3">
          <Link
            to="/checkin"
            onClick={onClose}
            className="text-sm text-fg-muted underline hover:text-fg"
          >
            去签到
          </Link>
          <Link
            to={`/switcher?accountId=${encodeURIComponent(row.accountId)}`}
            onClick={onClose}
            className="text-sm text-fg-muted underline hover:text-fg"
          >
            去换号
          </Link>
        </div>
      </aside>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-fg-subtle">{title}</h3>
      <div className="space-y-2 rounded-md border border-border bg-bg p-3">{children}</div>
    </div>
  );
}

function KV({ k, v }: { k: string; v: ReactNode }) {
  return (
    <div className="flex justify-between gap-4">
      <span className="shrink-0 text-fg-muted">{k}</span>
      <span className="text-right text-fg">{v}</span>
    </div>
  );
}
