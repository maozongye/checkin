/** 与架构 v1 IPC 合同对齐的前端类型（脱敏，无凭据明文） */

export type Platform = "workbuddy" | "trae_cn";

export type AccountStatus = "active" | "disabled" | "expired" | "error";

export type CheckinItemStatus =
  | "pending"
  | "success"
  | "already_done"
  | "failed"
  | "skipped"
  | "need_relogin";

export interface AccountPoints {
  balance: number;
  streakDays: number;
  fetchedAt: number;
}

export interface Account {
  id: string;
  platform: Platform;
  displayName: string;
  status: AccountStatus;
  tags: string[];
  points?: AccountPoints;
  /** 外部 uid，可空；永不含 token */
  externalUid?: string | null;
}

export interface CheckinRunAllResult {
  jobId: string;
}

export interface CheckinProgressEvent {
  jobId: string;
  done: number;
  total: number;
  currentAccount?: string;
  status?: CheckinItemStatus;
}

export interface CheckinItemResult {
  accountId: string;
  displayName: string;
  platform: Platform;
  status: CheckinItemStatus;
  message?: string;
  pointsDelta?: number;
}

export interface CheckinDoneEvent {
  jobId: string;
  success: number;
  failed: number;
  skipped?: number;
  results: CheckinItemResult[];
}

export interface SchedulerConfig {
  enabled: boolean;
  /** 每日时间，如 "09:00"；或 cron 表达式 */
  cronOrTime: string;
  timezone: string;
  platformsFilter?: Platform[] | null;
  lastRunAt?: number | null;
  nextRunAt?: number | null;
}

/* ---------- Monitor 看板合同（Architect 定稿 · Monitor v1） ---------- */

/** 会话健康 IPC 合同：仅 ok|expiring|expired|unknown（禁止 warn/error/need_relogin） */
export type SessionHealthStatus = "ok" | "expiring" | "expired" | "unknown";

export type AlertLevel = "info" | "warn" | "error";

export type AlertRuleId =
  | "checkin_fail"
  | "session_expired"
  | "session_expiring"
  | "points_expiring"
  | "points_low";

/** 最近签到：合同推荐 ok|fail|none */
export type MonitorCheckinStatus = "ok" | "fail" | "none" | CheckinItemStatus;

export interface MonitorSession {
  status: SessionHealthStatus;
  /** unix ms，可空 */
  expiresAt?: number | null;
  lastCheckedAt?: number | null;
}

export interface MonitorLastCheckin {
  status: MonitorCheckinStatus;
  at?: number | null;
  message?: string | null;
  pointsDelta?: number | null;
}

export interface MonitorAccountRow {
  accountId: string;
  displayName: string;
  /** 与 platform 同义，合同字段名 provider */
  provider: Platform;
  pointsBalance: number;
  streakDays: number;
  /** 积分过期时间 unix ms */
  pointsExpireAt?: number | null;
  session: MonitorSession;
  lastCheckin: MonitorLastCheckin;
}

export interface MonitorSummary {
  healthy: number;
  warn: number;
  error: number;
}

/**
 * monitor:alert 载荷（合同必填 rule / severity / dedupeKey；level/message 双写兼容）。
 */
export interface MonitorAlert {
  id: string;
  rule: AlertRuleId | string;
  accountId: string;
  severity: AlertLevel | string;
  title: string;
  body: string;
  at: number;
  dedupeKey: string;
  /** severity 别名 */
  level?: AlertLevel | string;
  /** body 别名 */
  message?: string;
  /** snake_case 兼容 */
  account_id?: string;
  dedupe_key?: string;
}

export interface MonitorDashboard {
  summary: MonitorSummary;
  /** unix ms */
  updatedAt: number;
  accounts: MonitorAccountRow[];
  alerts?: MonitorAlert[];
}

/** @deprecated 兼容别名，新代码用 MonitorDashboard */
export type MonitorDashboardLegacy = MonitorDashboard;

/* ---------- Switcher 合同（Architect 定稿） ---------- */

export type SwitchMessageKey =
  | "success"
  | "rolled_back"
  | "rollback_failed"
  | "trae_key_only"
  | "client_running"
  | "precheck_ok"
  | "need_relogin"
  | "target_credential_missing"
  | "cancelled"
  | "in_progress"
  | "failed";

/** switch://committed 载荷（camelCase；读侧兼容 snake_case） */
export interface SwitchCommittedEvent {
  accountId: string;
  product: string;
  snapshotId: string;
  account_id?: string;
  snapshot_id?: string;
}

export interface SwitchPrecheckResult {
  targetAccountId: string;
  targetDisplayName: string;
  provider: Platform;
  /** true → Confirm 禁用执行 */
  clientRunning: boolean;
  canProceed: boolean;
  warnings: SwitchMessageKey[];
  messageKey?: SwitchMessageKey;
  currentAccountId?: string | null;
}

export interface SwitchConfirmExecResult {
  jobId: string;
  messageKey: SwitchMessageKey;
  ok: boolean;
}

export interface SwitchProgressEvent {
  jobId: string;
  phase: "backup" | "write" | "verify" | "rollback" | "done";
  percent?: number;
  messageKey?: SwitchMessageKey;
  detail?: string;
}

export interface SwitchRollbackRetryResult {
  ok: boolean;
  messageKey: SwitchMessageKey;
}

/* ---------- Credential IPC（对齐 credential-tauri-ipc-draft · snake_case wire） ---------- */

export type MaterialKind = "workbuddy_tokens" | "trae_cn_icube_blob";

export type CredentialErrorCode =
  | "invalid_secret"
  | "replace_not_found"
  | "vault_dir_required"
  | "payload_too_large"
  | "backend_unavailable"
  | "backend_denied"
  | "not_found"
  | "invalid_ref"
  | "backup_required"
  | "corrupt_envelope"
  | "internal";

/** store 入参；secret_json 一次性，成功后调用方必须丢弃 */
export interface CredentialStoreArgs {
  platform: Platform;
  account_key: string;
  kind: MaterialKind;
  /** 一次性 secret JSON；api 层不缓存、不 console.log */
  secret_json: string;
  source_hint?: string;
  replace_ref?: string;
}

export interface CredentialStoreOk {
  credential_ref: string;
  kind: string;
  schema_version: number;
}

export interface CredentialDeleteArgs {
  credential_ref: string;
  backup_password: string;
  /** 可空；Rust 侧空则用 `{vault_dir}/backups` */
  backup_dir?: string;
}

export interface CredentialDeleteOk {
  deleted: boolean;
  backup_path: string;
}

/** message 禁止含 secret / token / cipher / file_key */
export interface CredentialIpcErr {
  code: CredentialErrorCode | string;
  message: string;
}

/* ---------- Credential meta（仅 list/put/get/delete meta，无 secret） ---------- */

export type CredentialMetaAction = "put" | "get" | "delete" | "list";

/** wire: snake_case；invoke 包在 { args } */
export interface CredentialMetaStoreArgs {
  action: CredentialMetaAction;
  account_id?: string;
  credential_ref?: string;
  label?: string;
}

export interface CredentialMetaItem {
  credential_ref: string;
  label: string;
  account_id: string;
}

export interface CredentialMetaStoreResult {
  ok?: boolean;
  credential_ref?: string;
  label?: string;
  account_id?: string;
  items?: CredentialMetaItem[];
}
