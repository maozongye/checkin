import { useEffect, useState } from "react";
import { PageHeader } from "@/components/layout/PageHeader";
import { CheckinButton } from "@/components/checkin/CheckinButton";
import { CheckinProgress } from "@/components/checkin/CheckinProgress";
import { useCheckinProgress } from "@/hooks/useCheckinProgress";
import { accountList, monitorGetDashboard } from "@/lib/api";
import type { Account, MonitorDashboard } from "@/lib/types";

export function OverviewPage() {
  const checkin = useCheckinProgress();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [dash, setDash] = useState<MonitorDashboard | null>(null);

  useEffect(() => {
    void accountList().then(setAccounts);
    void monitorGetDashboard().then(setDash);
  }, []);

  const active = accounts.filter((a) => a.status === "active").length;

  return (
    <div>
      <PageHeader
        title="概览"
        description="今日签到状态、账号健康与一键签到入口"
        actions={
          <CheckinButton loading={checkin.isRunning} onClick={() => void checkin.runAll()} />
        }
      />

      <div className="mb-6 grid gap-3 sm:grid-cols-3">
        <StatCard label="启用账号" value={String(active)} hint={`共 ${accounts.length} 个`} />
        <StatCard
          label="会话健康"
          value={dash ? String(dash.summary.healthy) : "—"}
          hint={
            dash
              ? `告警 ${dash.summary.warn} · 异常 ${dash.summary.error}`
              : "加载中"
          }
        />
        <StatCard
          label="积分合计"
          value={
            dash
              ? String(dash.accounts.reduce((s, a) => s + a.pointsBalance, 0))
              : "—"
          }
          hint="各平台余额汇总（缓存）"
        />
      </div>

      <section className="space-y-3">
        <h2 className="text-sm font-medium text-fg">一键签到</h2>
        <CheckinProgress
          phase={checkin.phase}
          done={checkin.done}
          total={checkin.total}
          currentAccount={checkin.currentAccount}
          status={checkin.status}
          summary={checkin.summary}
          error={checkin.error}
        />
      </section>
    </div>
  );
}

function StatCard({
  label,
  value,
  hint,
}: {
  label: string;
  value: string;
  hint: string;
}) {
  return (
    <div className="rounded-lg border border-border bg-bg-elevated p-4">
      <div className="text-xs text-fg-subtle">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums text-fg">{value}</div>
      <div className="mt-1 text-xs text-fg-muted">{hint}</div>
    </div>
  );
}
