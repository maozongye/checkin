import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import type { SwitchMessageKey, SwitchProgressEvent } from "@/lib/types";
import { SWITCH_PHASE_LABELS, tSwitch } from "@/lib/switchMessages";
import { cn } from "@/lib/utils";

export interface SwitchProgressProps {
  phase: "idle" | "running" | "done" | "error";
  progress: SwitchProgressEvent | null;
  resultKey?: SwitchMessageKey | null;
  error?: string | null;
  className?: string;
}

export function SwitchProgress({
  phase,
  progress,
  resultKey,
  error,
  className,
}: SwitchProgressProps) {
  if (phase === "idle") {
    return (
      <div
        className={cn(
          "rounded-lg border border-border bg-bg-elevated p-4 text-sm text-fg-muted",
          className,
        )}
      >
        选择目标账号并预检后，换号进度将显示在这里。
      </div>
    );
  }

  if (phase === "error") {
    return (
      <div
        className={cn(
          "rounded-lg border border-danger/40 bg-danger/10 p-4 text-sm text-danger",
          className,
        )}
      >
        {error ?? tSwitch("failed")}
      </div>
    );
  }

  const pct = progress?.percent ?? (phase === "done" ? 100 : 0);
  const tone =
    phase === "done"
      ? resultKey === "success"
        ? "success"
        : resultKey === "rolled_back"
          ? "warning"
          : "danger"
      : "default";

  // Progress 组件可能没有 warning tone，映射到 default/danger
  const progressTone = tone === "warning" ? "default" : tone;

  return (
    <div className={cn("space-y-3 rounded-lg border border-border bg-bg-elevated p-4", className)}>
      <div className="flex items-center justify-between gap-3 text-sm">
        <div className="min-w-0 text-fg">
          {phase === "running" && progress ? (
            <>
              <span className="text-fg-muted">阶段</span>{" "}
              <span className="font-medium">
                {SWITCH_PHASE_LABELS[progress.phase] ?? progress.phase}
              </span>
              {progress.messageKey ? (
                <Badge className="ml-2" variant="default">
                  {tSwitch(progress.messageKey)}
                </Badge>
              ) : null}
            </>
          ) : (
            <span className="font-medium">
              {resultKey ? tSwitch(resultKey) : "换号结束"}
            </span>
          )}
        </div>
        <span className="shrink-0 tabular-nums text-fg-muted">{pct}%</span>
      </div>

      <Progress value={pct} max={100} tone={progressTone} />

      {progress?.detail && phase === "running" ? (
        <p className="text-xs text-fg-subtle">{progress.detail}</p>
      ) : null}
    </div>
  );
}
