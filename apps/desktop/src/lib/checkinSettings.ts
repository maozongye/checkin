/** 签到 Runtime 门控：allow_live_http 默认关；关闭时仅 dry-run；开启才 live（checkin_run_all 透传） */

export const DEFAULT_ALLOW_LIVE_HTTP = false;

const STORAGE_KEY = "buddy.allow_live_http";

/** 变更时派发，供签到页等订阅 */
export const ALLOW_LIVE_HTTP_CHANGED_EVENT = "buddy:allow-live-http-changed";

export function getAllowLiveHttp(): boolean {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw == null || raw === "") return DEFAULT_ALLOW_LIVE_HTTP;
    return raw === "1" || raw === "true";
  } catch {
    return DEFAULT_ALLOW_LIVE_HTTP;
  }
}

/** @returns 保存后的值 */
export function setAllowLiveHttp(enabled: boolean): boolean {
  const next = Boolean(enabled);
  try {
    localStorage.setItem(STORAGE_KEY, next ? "true" : "false");
    window.dispatchEvent(
      new CustomEvent(ALLOW_LIVE_HTTP_CHANGED_EVENT, { detail: next }),
    );
  } catch {
    /* localStorage 不可用时仍返回意图值 */
  }
  return next;
}

/** 关闭 allow_live_http 时视为 dry-run（与 checkin-core Runtime 门控语义对齐） */
export function isCheckinDryRun(): boolean {
  return !getAllowLiveHttp();
}
