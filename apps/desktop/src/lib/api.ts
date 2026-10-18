/**
 * Tauri invoke 封装。开发态无 Tauri 时走 mock，便于纯 Vite 预览。
 * 不做真实签到；凭据走 Auth IPC（store/delete），不构造 CredentialStore。
 */
import type {
  Account,
  CheckinDoneEvent,
  CheckinProgressEvent,
  CheckinRunAllResult,
  CredentialDeleteArgs,
  CredentialDeleteOk,
  CredentialMetaStoreArgs,
  CredentialMetaStoreResult,
  CredentialStoreArgs,
  CredentialStoreOk,
  MonitorAccountRow,
  MonitorAlert,
  MonitorDashboard,
  SchedulerConfig,
  SessionHealthStatus,
  SwitchConfirmExecResult,
  SwitchPrecheckResult,
  SwitchProgressEvent,
  SwitchRollbackRetryResult,
} from "./types";
import { parseCredentialInvokeError } from "./credentialErrors";
import { getAllowLiveHttp } from "./checkinSettings";

const COMMANDS = {
  accountList: "account_list",
  checkinRunAll: "checkin_run_all",
  schedulerGet: "scheduler_get",
  schedulerSet: "scheduler_set",
  switchPrecheck: "switch_precheck",
  switchConfirmExec: "switch_confirm_exec",
  switchRollbackRetry: "switch_rollback_retry",
  switchList: "switch_list",
  monitorGetDashboard: "monitor_get_dashboard",
  monitorRefresh: "monitor_refresh",
  credentialStore: "credential_store",
  credentialDelete: "credential_delete",
  credentialMetaStore: "credential_meta_store",
} as const;

export const EVENTS = {
  checkinProgress: "checkin:progress",
  checkinDone: "checkin:done",
  monitorAlert: "monitor:alert",
  /** 字面量含 `://`，与合同一致 */
  switchProgress: "switch://progress",
  /** 换号成功提交：accountId / product / snapshotId */
  switchCommitted: "switch://committed",
} as const;

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    return mockInvoke<T>(cmd, args);
  }
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  return tauriInvoke<T>(cmd, args);
}

type Unlisten = () => void;

export async function listen<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<Unlisten> {
  if (!isTauri()) {
    return mockListen<T>(event, handler);
  }
  const { listen: tauriListen } = await import("@tauri-apps/api/event");
  const unlisten = await tauriListen<T>(event, (e) => handler(e.payload));
  return unlisten;
}

/* ---------- public API ---------- */

export function accountList(): Promise<Account[]> {
  return invoke<Account[]>(COMMANDS.accountList);
}

export function checkinRunAll(opts?: {
  platform?: "workbuddy" | "trae_cn" | "all";
  /** 覆盖 Settings；默认读 allow_live_http（关=dry-run） */
  allowLiveHttp?: boolean;
  /** 显式 dry_run；默认 = !allowLiveHttp */
  dryRun?: boolean;
}): Promise<CheckinRunAllResult> {
  const allowLiveHttp = opts?.allowLiveHttp ?? getAllowLiveHttp();
  const dryRun = opts?.dryRun ?? !allowLiveHttp;
  return invoke<CheckinRunAllResult>(COMMANDS.checkinRunAll, {
    platform: opts?.platform,
    // snake_case 对齐 Rust command 参数名；camelCase 兼容
    allow_live_http: allowLiveHttp,
    allowLiveHttp,
    dry_run: dryRun,
    dryRun,
  });
}

export function schedulerGet(): Promise<SchedulerConfig> {
  return invoke<SchedulerConfig>(COMMANDS.schedulerGet);
}

export function schedulerSet(config: SchedulerConfig): Promise<SchedulerConfig> {
  return invoke<SchedulerConfig>(COMMANDS.schedulerSet, { config });
}

export function switchList(): Promise<Account[]> {
  return invoke<Account[]>(COMMANDS.switchList);
}

export function switchPrecheck(
  accountId: string,
  opts?: { forceRunning?: boolean },
): Promise<SwitchPrecheckResult> {
  const forceRunning =
    opts?.forceRunning ??
    (typeof window !== "undefined" &&
      new URLSearchParams(window.location.search).get("forceRunning") === "1");
  return invoke<SwitchPrecheckResult>(COMMANDS.switchPrecheck, {
    accountId,
    forceRunning: forceRunning || undefined,
  });
}

export function switchConfirmExec(accountId: string): Promise<SwitchConfirmExecResult> {
  return invoke<SwitchConfirmExecResult>(COMMANDS.switchConfirmExec, { accountId });
}

export function switchRollbackRetry(jobId: string): Promise<SwitchRollbackRetryResult> {
  return invoke<SwitchRollbackRetryResult>(COMMANDS.switchRollbackRetry, { jobId });
}

export function monitorGetDashboard(): Promise<MonitorDashboard> {
  return invoke<MonitorDashboard>(COMMANDS.monitorGetDashboard);
}

/** 主动刷新健康/积分缓存，返回最新 dashboard */
export function monitorRefresh(): Promise<MonitorDashboard> {
  return invoke<MonitorDashboard>(COMMANDS.monitorRefresh);
}

/**
 * 写入凭据（Auth IPC 草案 · snake_case wire）。成功后调用方应丢弃 secret_json。
 * api 层不缓存 secret、不 console.log。
 * 前端绝不构造 CredentialStore / with_vault_dir（vault 由 Auth 注入）。
 */
export async function storeCredential(args: CredentialStoreArgs): Promise<CredentialStoreOk> {
  try {
    // 仅透传；不把 secret_json 存进模块级状态
    return await invoke<CredentialStoreOk>(COMMANDS.credentialStore, { args });
  } catch (e) {
    const err = parseCredentialInvokeError(e);
    throw Object.assign(new Error(err.message), { code: err.code, ipc: err });
  }
}

/** 删除凭据（需备份口令）；不回传 secret。 */
export async function deleteCredential(args: CredentialDeleteArgs): Promise<CredentialDeleteOk> {
  try {
    return await invoke<CredentialDeleteOk>(COMMANDS.credentialDelete, { args });
  } catch (e) {
    const err = parseCredentialInvokeError(e);
    throw Object.assign(new Error(err.message), { code: err.code, ipc: err });
  }
}

/**
 * 壳层凭据 **meta**（list/put/get/delete）：无 secret。
 * Wire：`invoke("credential_meta_store", { args: { action, account_id?, credential_ref?, label? } })`（snake_case）。
 * 真导入请用 storeCredential / deleteCredential。
 */
export function credentialMetaStore(
  args: CredentialMetaStoreArgs,
): Promise<CredentialMetaStoreResult> {
  return invoke<CredentialMetaStoreResult>(COMMANDS.credentialMetaStore, { args });
}

/** 列出凭据 meta（无 secret） */
export function listCredentialMeta(): Promise<CredentialMetaStoreResult> {
  return credentialMetaStore({ action: "list" });
}

/**
 * @deprecated 旧扁平 action API；请用 credentialMetaStore（meta）或 storeCredential（真 vault）。
 */
export function credentialStore(_args: never): never {
  throw new Error(
    "credentialStore 已废弃；meta 用 credentialMetaStore，真导入用 storeCredential / deleteCredential",
  );
}

/* ---------- mock ---------- */

/** Vite-only：凭据 meta 内存表（无 secret） */
let mockCredentialMeta: Array<{
  credential_ref: string;
  label: string;
  account_id: string;
}> = [
  {
    credential_ref: "cred://v1/account/mock-wb-1",
    label: "WB · 小强",
    account_id: "acc-wb-1",
  },
  {
    credential_ref: "cred://v1/account/mock-trae-1",
    label: "Trae · 主号",
    account_id: "acc-trae-1",
  },
];

const MOCK_ACCOUNTS: Account[] = [
  {
    id: "acc-wb-1",
    platform: "workbuddy",
    displayName: "WB · 小强",
    status: "active",
    tags: ["日常"],
    points: { balance: 1280, streakDays: 12, fetchedAt: Date.now() - 3600_000 },
    externalUid: "wb_1001",
  },
  {
    id: "acc-wb-2",
    platform: "workbuddy",
    displayName: "WB · 备用",
    status: "active",
    tags: ["备用"],
    points: { balance: 420, streakDays: 3, fetchedAt: Date.now() - 7200_000 },
  },
  {
    id: "acc-trae-1",
    platform: "trae_cn",
    displayName: "Trae · 主号",
    status: "active",
    tags: ["IDE"],
    points: { balance: 860, streakDays: 7, fetchedAt: Date.now() - 1800_000 },
  },
  {
    id: "acc-trae-2",
    platform: "trae_cn",
    displayName: "Trae · 需重登",
    status: "expired",
    tags: ["告警"],
    points: { balance: 0, streakDays: 0, fetchedAt: Date.now() - 86_400_000 },
  },
];

let mockScheduler: SchedulerConfig = {
  enabled: false,
  cronOrTime: "09:00",
  timezone: "Asia/Shanghai",
  platformsFilter: null,
  lastRunAt: null,
  nextRunAt: null,
};

type MockHandler = (payload: unknown) => void;
const mockHandlers = new Map<string, Set<MockHandler>>();

function mockListen<T>(event: string, handler: (payload: T) => void): Unlisten {
  let set = mockHandlers.get(event);
  if (!set) {
    set = new Set();
    mockHandlers.set(event, set);
  }
  const wrapped: MockHandler = (p) => handler(p as T);
  set.add(wrapped);
  return () => {
    set?.delete(wrapped);
  };
}

function emitMock(event: string, payload: unknown): void {
  const set = mockHandlers.get(event);
  if (!set) return;
  for (const h of set) h(payload);
}

function buildMockDashboard(): MonitorDashboard {
  const now = Date.now();
  const accounts: MonitorAccountRow[] = [
    {
      accountId: "acc-wb-1",
      displayName: "WB · 小强",
      provider: "workbuddy" as const,
      pointsBalance: 1280,
      streakDays: 12,
      pointsExpireAt: now + 30 * 86_400_000,
      session: {
        status: "ok" as const,
        expiresAt: now + 14 * 86_400_000,
        lastCheckedAt: now - 600_000,
      },
      lastCheckin: {
        status: "ok" as const,
        at: now - 3_600_000,
        message: "签到成功 +10",
        pointsDelta: 10,
      },
    },
    {
      accountId: "acc-wb-2",
      displayName: "WB · 备用",
      provider: "workbuddy" as const,
      pointsBalance: 420,
      streakDays: 3,
      pointsExpireAt: now + 10 * 86_400_000,
      session: {
        status: "expiring" as const,
        expiresAt: now + 2 * 86_400_000,
        lastCheckedAt: now - 1_200_000,
      },
      lastCheckin: {
        status: "fail" as const,
        at: now - 86_400_000,
        message: "网络超时（mock）",
        pointsDelta: 0,
      },
    },
    {
      accountId: "acc-trae-1",
      displayName: "Trae · 主号",
      provider: "trae_cn" as const,
      pointsBalance: 860,
      streakDays: 7,
      pointsExpireAt: now + 20 * 86_400_000,
      session: {
        status: "ok" as const,
        expiresAt: now + 21 * 86_400_000,
        lastCheckedAt: now - 300_000,
      },
      lastCheckin: {
        status: "ok" as const,
        at: now - 8_000_000,
        message: "今日已签",
        pointsDelta: 0,
      },
    },
    {
      accountId: "acc-trae-2",
      displayName: "Trae · 需重登",
      provider: "trae_cn" as const,
      pointsBalance: 0,
      streakDays: 0,
      pointsExpireAt: null,
      session: {
        status: "expired" as const,
        expiresAt: now - 86_400_000,
        lastCheckedAt: now - 3_600_000,
      },
      lastCheckin: {
        status: "fail" as const,
        at: now - 2 * 86_400_000,
        message: "登录态失效，请重新登录",
        pointsDelta: null,
      },
    },
  ];

  let healthy = 0;
  let warn = 0;
  let error = 0;
  for (const a of accounts) {
    const s: SessionHealthStatus = a.session.status;
    if (s === "ok") healthy += 1;
    else if (s === "expiring" || s === "unknown") warn += 1;
    else error += 1; // expired
  }

  return {
    summary: { healthy, warn, error },
    updatedAt: now,
    accounts,
    alerts: [
      {
        id: "alert-1",
        rule: "session_expired",
        accountId: "acc-trae-2",
        severity: "error",
        title: "Trae · 需重登：登录态已失效",
        body: "会话已过期，请重新登录",
        level: "error",
        message: "会话已过期，请重新登录",
        dedupeKey: "session_expired:acc-trae-2:expired",
        at: now - 3_600_000,
      },
      {
        id: "alert-2",
        rule: "checkin_fail",
        accountId: "acc-wb-2",
        severity: "error",
        title: "WB · 备用：签到失败",
        body: "上次签到失败",
        level: "error",
        message: "上次签到失败",
        dedupeKey: "checkin_fail:acc-wb-2:mock",
        at: now - 86_400_000,
      },
    ],
  };
}

async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  await delay(120 + Math.random() * 180);

  switch (cmd) {
    case COMMANDS.accountList:
      return MOCK_ACCOUNTS as T;

    case COMMANDS.checkinRunAll: {
      const jobId = `job-${Date.now()}`;
      const allow =
        args?.allow_live_http === true ||
        args?.allowLiveHttp === true;
      const dry =
        args?.dry_run === true ||
        args?.dryRun === true ||
        !allow;
      void simulateCheckin(jobId, { dryRun: dry });
      return { jobId } as T;
    }

    case COMMANDS.schedulerGet:
      return { ...mockScheduler } as T;

    case COMMANDS.schedulerSet: {
      mockScheduler = { ...(args?.config as SchedulerConfig) };
      return { ...mockScheduler } as T;
    }

    case COMMANDS.switchList: {
      // mock：基础账号 + credential_meta 中有 cred://v1/account/ 的导入号
      const base = MOCK_ACCOUNTS.filter((a) => a.status !== "disabled");
      const byId = new Map(base.map((a) => [a.id, a]));
      for (const m of mockCredentialMeta) {
        if (!m.credential_ref.startsWith("cred://v1/account/")) continue;
        if (byId.has(m.account_id)) continue;
        byId.set(m.account_id, {
          id: m.account_id,
          platform: "workbuddy",
          displayName: m.label || m.account_id,
          status: "active",
          tags: ["imported"],
        });
      }
      return [...byId.values()] as T;
    }

    case COMMANDS.switchPrecheck: {
      const accountId = String(args?.accountId ?? args?.account_id ?? "");
      let target = MOCK_ACCOUNTS.find((a) => a.id === accountId);
      if (!target) {
        const m = mockCredentialMeta.find((c) => c.account_id === accountId);
        if (m) {
          target = {
            id: m.account_id,
            platform: "workbuddy",
            displayName: m.label || m.account_id,
            status: "active",
            tags: ["imported"],
          };
        }
      }
      if (!target) {
        throw new Error("账号不存在");
      }
      const forceRunning =
        Boolean(args?.forceRunning) ||
        (typeof window !== "undefined" &&
          new URLSearchParams(window.location.search).get("forceRunning") === "1");
      // 轮换：acc-wb-2 默认视为客户端在跑；opts.forceRunning / ?forceRunning=1 强制任意账号
      const clientRunning = forceRunning || accountId === "acc-wb-2";
      const warnings: SwitchPrecheckResult["warnings"] = [];
      if (target.platform === "trae_cn") {
        warnings.push("trae_key_only");
      }
      const credMeta = mockCredentialMeta.find(
        (c) =>
          c.account_id === accountId &&
          c.credential_ref.startsWith("cred://v1/account/"),
      );
      const credMissing = !credMeta;
      if (credMissing) warnings.push("target_credential_missing");
      const needRelogin = target.status === "expired" || target.status === "error";
      // Vite mock 对齐 Rust mock_scenarios：缺凭据进 warnings，但不阻断已知 mock 账号
      const blockMissingCred = false;
      const canProceed = !clientRunning && !needRelogin && !blockMissingCred;
      const result: SwitchPrecheckResult = {
        targetAccountId: target.id,
        targetDisplayName: target.displayName,
        provider: target.platform,
        clientRunning,
        canProceed,
        warnings,
        messageKey: credMissing && blockMissingCred
          ? "target_credential_missing"
          : needRelogin
            ? "need_relogin"
            : clientRunning
              ? "client_running"
              : "precheck_ok",
        currentAccountId: "acc-wb-1",
      };
      return result as T;
    }

    case COMMANDS.switchConfirmExec: {
      const accountId = String(args?.accountId ?? args?.account_id ?? "");
      let target = MOCK_ACCOUNTS.find((a) => a.id === accountId);
      if (!target) {
        const m = mockCredentialMeta.find((c) => c.account_id === accountId);
        if (m) {
          target = {
            id: m.account_id,
            platform: "workbuddy",
            displayName: m.label || m.account_id,
            status: "active",
            tags: ["imported"],
          };
        }
      }
      if (!target) throw new Error("账号不存在");
      // 真路径：无 cred://v1/account/ 则 target_credential_missing
      const hasCred = mockCredentialMeta.some(
        (c) =>
          c.account_id === accountId &&
          c.credential_ref.startsWith("cred://v1/account/"),
      );
      // Vite mock 场景账号放行；纯导入号必须有凭据
      const isMockScenario = accountId === "acc-wb-2" || accountId === "acc-trae-2";
      if (!hasCred && !isMockScenario) {
        throw new Error("target_credential_missing");
      }
      const jobId = `switch-${Date.now()}`;
      void simulateSwitch(jobId, accountId);
      // 立即返回 in_progress；最终 messageKey 由 progress done 事件带出
      const result: SwitchConfirmExecResult = {
        jobId,
        messageKey: "in_progress",
        ok: true,
      };
      return result as T;
    }

    case COMMANDS.switchRollbackRetry: {
      const result: SwitchRollbackRetryResult = {
        ok: true,
        messageKey: "rolled_back",
      };
      return result as T;
    }

    case COMMANDS.monitorGetDashboard:
      return buildMockDashboard() as T;

    case COMMANDS.monitorRefresh: {
      const dash = buildMockDashboard();
      // 刷新后偶发推一条 alert，便于 UI 联调
      const alert: MonitorAlert = {
        id: `alert-${Date.now()}`,
        rule: "checkin_fail",
        accountId: "acc-wb-2",
        severity: "error",
        title: "WB · 备用：签到失败",
        body: "网络超时（mock）",
        level: "error",
        message: "网络超时（mock）",
        dedupeKey: "checkin_fail:acc-wb-2:mock",
        at: Date.now(),
      };
      queueMicrotask(() => emitMock(EVENTS.monitorAlert, alert));
      return dash as T;
    }

    case COMMANDS.credentialStore: {
      // Vite mock：假返回 credential_ref；**不要**把 secret_json 写进内存
      const storeArgs = (args?.args ?? args) as Record<string, unknown> | undefined;
      const platform = String(storeArgs?.platform ?? "");
      const accountKey = String(storeArgs?.account_key ?? "");
      const kind = String(storeArgs?.kind ?? "");
      const secretJson = String(storeArgs?.secret_json ?? "");
      // 刻意丢弃内容：只校验非空，不保存
      if (!platform || !accountKey || !kind) {
        throw { code: "invalid_secret", message: "platform/account_key/kind required" };
      }
      if (!secretJson.trim()) {
        throw { code: "invalid_secret", message: "secret_json required" };
      }
      const slug = accountKey.replace(/[^a-zA-Z0-9]+/g, "-");
      const cref = `cred://v1/account/mock-${slug}`;
      // 导入后写入 meta，换号选择器可立刻看到
      const aid = accountKey;
      mockCredentialMeta = mockCredentialMeta.filter(
        (c) => c.account_id !== aid && c.credential_ref !== cref,
      );
      mockCredentialMeta.push({
        credential_ref: cref,
        label: accountKey,
        account_id: aid,
      });
      return {
        credential_ref: cref,
        kind,
        schema_version: 1,
      } satisfies CredentialStoreOk as T;
    }

    case COMMANDS.credentialDelete: {
      const delArgs = (args?.args ?? args) as Record<string, unknown> | undefined;
      const credentialRef = String(delArgs?.credential_ref ?? "");
      const backupPassword = String(delArgs?.backup_password ?? "");
      const backupDir =
        delArgs?.backup_dir != null && String(delArgs.backup_dir).trim()
          ? String(delArgs.backup_dir)
          : "/mock/backups";
      if (!credentialRef.startsWith("cred://")) {
        throw { code: "invalid_ref", message: "credential_ref invalid" };
      }
      if (!backupPassword.trim()) {
        throw { code: "backup_required", message: "backup_password required" };
      }
      const leaf = credentialRef.split("/").pop() ?? "unknown";
      return {
        deleted: true,
        backup_path: `${backupDir.replace(/\/$/, "")}/mock-${leaf}.cred.bak`,
      } satisfies CredentialDeleteOk as T;
    }


    case COMMANDS.credentialMetaStore: {
      const metaArgs = (args?.args ?? args) as Record<string, unknown> | undefined;
      const action = String(metaArgs?.action ?? "");
      if (action === "list") {
        return { ok: true, items: [...mockCredentialMeta] } as T;
      }
      if (action === "get") {
        const ref = metaArgs?.credential_ref != null ? String(metaArgs.credential_ref) : "";
        const aid = metaArgs?.account_id != null ? String(metaArgs.account_id) : "";
        const found = mockCredentialMeta.find(
          (c) => (ref && c.credential_ref === ref) || (aid && c.account_id === aid),
        );
        if (!found) throw new Error("credential not found");
        return {
          ok: true,
          credential_ref: found.credential_ref,
          label: found.label,
          account_id: found.account_id,
        } as T;
      }
      if (action === "put") {
        const aid = String(metaArgs?.account_id ?? "");
        if (!aid) throw new Error("put 需要 account_id");
        const label = String(metaArgs?.label ?? `cred-${aid}`);
        const cref = String(
          metaArgs?.credential_ref ?? `cred://v1/account/mock-${aid}`,
        );
        mockCredentialMeta = mockCredentialMeta.filter(
          (c) => c.account_id !== aid && c.credential_ref !== cref,
        );
        mockCredentialMeta.push({ credential_ref: cref, label, account_id: aid });
        return { ok: true, credential_ref: cref, label, account_id: aid } as T;
      }
      if (action === "delete") {
        const ref = metaArgs?.credential_ref != null ? String(metaArgs.credential_ref) : "";
        const aid = metaArgs?.account_id != null ? String(metaArgs.account_id) : "";
        const before = mockCredentialMeta.length;
        mockCredentialMeta = mockCredentialMeta.filter(
          (c) => !((ref && c.credential_ref === ref) || (aid && c.account_id === aid)),
        );
        return {
          ok: mockCredentialMeta.length < before,
          credential_ref: ref || undefined,
          account_id: aid || undefined,
        } as T;
      }
      throw new Error(`unknown credential_meta_store action: ${action}`);
    }


    default:
      throw new Error(`[mock] unknown command: ${cmd}`);
  }
}

async function simulateCheckin(
  jobId: string,
  opts?: { dryRun?: boolean },
): Promise<void> {
  const dryRun = opts?.dryRun ?? true;
  const targets = MOCK_ACCOUNTS.filter((a) => a.status === "active");
  const total = targets.length;
  const results: CheckinDoneEvent["results"] = [];

  for (let i = 0; i < total; i++) {
    await delay(450);
    const acc = targets[i]!;
    let status: CheckinProgressEvent["status"];
    let message: string;
    let pointsDelta = 0;
    if (dryRun) {
      status = "skipped";
      message = "dry_run";
    } else {
      const ok = acc.id !== "acc-wb-2" || Math.random() > 0.35;
      status = ok ? (i === 1 ? "already_done" : "success") : "failed";
      message = ok
        ? status === "already_done"
          ? "今日已签"
          : "签到成功"
        : "网络超时（mock）";
      pointsDelta = status === "success" ? 10 : 0;
    }
    const progress: CheckinProgressEvent = {
      jobId,
      done: i + 1,
      total,
      currentAccount: acc.displayName,
      status,
    };
    emitMock(EVENTS.checkinProgress, progress);
    results.push({
      accountId: acc.id,
      displayName: acc.displayName,
      platform: acc.platform,
      status: status ?? "skipped",
      message,
      pointsDelta,
    });
  }

  const success = results.filter(
    (r) => r.status === "success" || r.status === "already_done",
  ).length;
  const failed = results.filter((r) => r.status === "failed").length;
  const done: CheckinDoneEvent = {
    jobId,
    success,
    failed,
    skipped: results.filter((r) => r.status === "skipped").length,
    results,
  };
  emitMock(EVENTS.checkinDone, done);
}

/** mock 换号进度；偶发失败路径：acc-trae-2 → rollback_failed；acc-wb-2 若强行执行 → rolled_back */
async function simulateSwitch(jobId: string, accountId: string): Promise<void> {
  const phases: SwitchProgressEvent["phase"][] = ["backup", "write", "verify"];
  const failHard = accountId === "acc-trae-2";
  const failSoft = accountId === "acc-wb-2";

  for (let i = 0; i < phases.length; i++) {
    await delay(380);
    const phase = phases[i]!;
    const evt: SwitchProgressEvent = {
      jobId,
      phase,
      percent: Math.round(((i + 1) / (phases.length + 1)) * 100),
      messageKey: "in_progress",
      detail: phase,
    };
    emitMock(EVENTS.switchProgress, evt);

    if (failSoft && phase === "write") {
      await delay(300);
      emitMock(EVENTS.switchProgress, {
        jobId,
        phase: "rollback",
        percent: 85,
        messageKey: "in_progress",
        detail: "write failed, rolling back",
      } satisfies SwitchProgressEvent);
      await delay(350);
      emitMock(EVENTS.switchProgress, {
        jobId,
        phase: "done",
        percent: 100,
        messageKey: "rolled_back",
        detail: "rolled back",
      } satisfies SwitchProgressEvent);
      return;
    }

    if (failHard && phase === "verify") {
      await delay(300);
      emitMock(EVENTS.switchProgress, {
        jobId,
        phase: "rollback",
        percent: 90,
        messageKey: "in_progress",
        detail: "verify failed, rollback attempting",
      } satisfies SwitchProgressEvent);
      await delay(400);
      emitMock(EVENTS.switchProgress, {
        jobId,
        phase: "done",
        percent: 100,
        messageKey: "rollback_failed",
        detail: "rollback failed",
      } satisfies SwitchProgressEvent);
      return;
    }
  }

  await delay(280);
  emitMock(EVENTS.switchProgress, {
    jobId,
    phase: "done",
    percent: 100,
    messageKey: "success",
    detail: "ok",
  } satisfies SwitchProgressEvent);
  const target = MOCK_ACCOUNTS.find((a) => a.id === accountId);
  emitMock(EVENTS.switchCommitted, {
    accountId,
    product: target?.platform ?? "workbuddy",
    snapshotId: `snap-mock-${Date.now()}`,
  });
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
