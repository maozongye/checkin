import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import type { CheckinDoneEvent, CheckinItemStatus } from "@/lib/types";
import { cn } from "@/lib/utils";

export interface CheckinProgressProps {
  done: number;
  total: number;
  currentAccount?: string;
  status?: CheckinItemStatus;
  summary?: CheckinDoneEvent | null;
  phase: "idle" | "running" | "done" | "error";
  error?: string | null;
  className?: string;
}

function statusLabel(s?: CheckinItemStatus): string {
  switch (s) {
    case "success":
      return "成功";
    case "already_done":
      return "已签";
    case "failed":
      return "失败";
    case "skipped":
      return "跳过";
    case "need_relogin":
      return "需重登";
    case "pending":
      return "进行中";
    default:
      return "";
  }
}

export function CheckinProgress({
  done,
  total,
  currentAccount,
  status,
  summary,
  phase,
  error,
  className,
}: CheckinProgressProps) {
  if (phase === "idle") {
    return (
      <div className={cn("rounded-lg border border-border bg-bg-elevated p-4 text-sm text-fg-muted", className)}>
        尚未开始签到。点击「一键签到」后将显示进度。
      </div>
    );
  }

  if (phase === "error") {
    return (
      <div className={cn("rounded-lg border border-danger/40 bg-danger/10 p-4 text-sm text-danger", className)}>
        签到启动失败：{error ?? "未知错误"}
      </div>
    );
  }

  const tone =
    phase === "done"
      ? summary && summary.failed > 0
        ? "danger"
        : "success"
      : "default";

  return (
    <div className={cn("space-y-3 rounded-lg border border-border bg-bg-elevated p-4", className)}>
      <div className="flex items-center justify-between gap-3 text-sm">
        <div className="min-w-0 text-fg">
          {phase === "running" ? (
            <>
              <span className="text-fg-muted">正在处理</span>{" "}
              <span className="font-medium">{currentAccount ?? "…"}</span>
              {status ? (
                <Badge className="ml-2" variant={status === "failed" ? "danger" : "default"}>
                  {statusLabel(status)}
                </Badge>
              ) : null}
            </>
          ) : (
            <span className="font-medium">签到完成</span>
          )}
        </div>
        <span className="shrink-0 tabular-nums text-fg-muted">
          {done}/{total || "—"}
        </span>
      </div>

      <Progress value={done} max={Math.max(total, 1)} tone={tone} />

      {summary ? (
        <div className="flex flex-wrap gap-2 text-xs text-fg-muted">
          <Badge variant="success">成功 {summary.success}</Badge>
          <Badge variant="danger">失败 {summary.failed}</Badge>
          {summary.skipped != null ? (
            <Badge variant="default">跳过 {summary.skipped}</Badge>
          ) : null}
        </div>
      ) : null}

      {summary?.results?.length ? (
        <ul className="max-h-40 space-y-1 overflow-auto text-xs text-fg-muted">
          {summary.results.map((r) => (
            <li key={r.accountId} className="flex justify-between gap-2 border-t border-border/60 pt-1">
              <span className="truncate text-fg">{r.displayName}</span>
              <span className={r.status === "failed" ? "text-danger" : "text-fg-subtle"}>
                {statusLabel(r.status)}
                {r.message ? ` · ${r.message}` : ""}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
