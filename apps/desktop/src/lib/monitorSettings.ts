/** 探活间隔：壳层按此分钟数调用 monitor_refresh（无常驻 Rust timer） */

export const DEFAULT_HEALTH_INTERVAL_MINUTES = 30;

const STORAGE_KEY = "buddy.health_interval_minutes";

/** 变更时派发，供 AppShell 轮询钩子换 timer */
export const HEALTH_INTERVAL_CHANGED_EVENT = "buddy:health-interval-changed";

const MIN_MINUTES = 5;
const MAX_MINUTES = 1440;

export function clampHealthIntervalMinutes(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_HEALTH_INTERVAL_MINUTES;
  return Math.min(MAX_MINUTES, Math.max(MIN_MINUTES, Math.round(n)));
}

export function getHealthIntervalMinutes(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw == null || raw === "") return DEFAULT_HEALTH_INTERVAL_MINUTES;
    const n = Number(raw);
    return clampHealthIntervalMinutes(n);
  } catch {
    return DEFAULT_HEALTH_INTERVAL_MINUTES;
  }
}

export function setHealthIntervalMinutes(n: number): number {
  const clamped = clampHealthIntervalMinutes(n);
  try {
    localStorage.setItem(STORAGE_KEY, String(clamped));
    window.dispatchEvent(
      new CustomEvent(HEALTH_INTERVAL_CHANGED_EVENT, { detail: clamped }),
    );
  } catch {
    /* localStorage 不可用时仍返回钳制值 */
  }
  return clamped;
}
