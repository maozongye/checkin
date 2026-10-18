import { useEffect, useRef } from "react";
import { monitorRefresh } from "@/lib/api";
import {
  getHealthIntervalMinutes,
  HEALTH_INTERVAL_CHANGED_EVENT,
} from "@/lib/monitorSettings";

/**
 * 壳层按 settings 探活间隔调用 monitor_refresh。
 * 首次进入不立刻 refresh（MonitorPage 已有手动刷新）；仅按间隔调。
 * 失败静默 console.warn，不弹窗。
 */
export function useMonitorHealthPoll(): void {
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    const clear = () => {
      if (timerRef.current != null) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };

    const tick = () => {
      if (typeof document !== "undefined" && document.visibilityState === "hidden") {
        return;
      }
      void monitorRefresh().catch((e) => {
        console.warn("[monitor] health poll refresh failed", e);
      });
    };

    const arm = () => {
      clear();
      const minutes = getHealthIntervalMinutes();
      const ms = minutes * 60_000;
      timerRef.current = setInterval(tick, ms);
    };

    arm();

    const onChanged = () => arm();
    window.addEventListener(HEALTH_INTERVAL_CHANGED_EVENT, onChanged);

    return () => {
      clear();
      window.removeEventListener(HEALTH_INTERVAL_CHANGED_EVENT, onChanged);
    };
  }, []);
}
