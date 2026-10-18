import * as React from "react";
import { cn } from "@/lib/utils";

export interface ProgressProps extends React.HTMLAttributes<HTMLDivElement> {
  value: number;
  max?: number;
  /** success | danger | default */
  tone?: "default" | "success" | "danger";
}

export function Progress({
  value,
  max = 100,
  tone = "default",
  className,
  ...props
}: ProgressProps) {
  const pct = max <= 0 ? 0 : Math.min(100, Math.max(0, (value / max) * 100));
  const bar =
    tone === "success"
      ? "bg-success"
      : tone === "danger"
        ? "bg-danger"
        : "bg-accent";

  return (
    <div
      role="progressbar"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
      className={cn("h-2 w-full overflow-hidden rounded-full bg-border", className)}
      {...props}
    >
      <div
        className={cn("h-full rounded-full transition-all duration-300 ease-out", bar)}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}
