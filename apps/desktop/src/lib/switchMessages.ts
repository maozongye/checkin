import type { SwitchMessageKey } from "./types";

/** message_key → 中文文案；禁止把英文 key 直接展示给用户 */
export const SWITCH_MESSAGES: Record<SwitchMessageKey, string> = {
  success: "换号成功",
  rolled_back: "换号失败，已回滚到原登录态",
  rollback_failed: "换号失败且回滚未完成，请重试回滚或手动检查客户端登录态",
  trae_key_only: "Trae 仅写入密钥/会话，不迁移 Chat 与工作区",
  client_running: "目标平台客户端正在运行，请先完全退出后再换号",
  precheck_ok: "预检通过，可以换号",
  need_relogin: "目标账号需要重新登录后再换号",
  target_credential_missing: "目标账号缺少凭据（需先导入 cred://v1/account/...）",
  cancelled: "已取消换号",
  in_progress: "换号进行中…",
  failed: "换号失败",
};

export function tSwitch(key: SwitchMessageKey | undefined | null, fallback?: string): string {
  if (!key) return fallback ?? "";
  return SWITCH_MESSAGES[key] ?? fallback ?? String(key);
}

export const SWITCH_PHASE_LABELS: Record<
  "backup" | "write" | "verify" | "rollback" | "done",
  string
> = {
  backup: "备份当前登录态",
  write: "写入目标账号",
  verify: "校验登录态",
  rollback: "回滚中",
  done: "完成",
};
