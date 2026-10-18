import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { PageHeader } from "@/components/layout/PageHeader";
import { ConfirmSwitchDialog } from "@/components/switcher/ConfirmSwitchDialog";
import { SwitchProgress } from "@/components/switcher/SwitchProgress";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  EVENTS,
  listen,
  switchConfirmExec,
  switchList,
  switchPrecheck,
  switchRollbackRetry,
  accountList,
  listCredentialMeta,
} from "@/lib/api";
import type {
  Account,
  CredentialMetaItem,
  SwitchCommittedEvent,
  SwitchMessageKey,
  SwitchPrecheckResult,
  SwitchProgressEvent,
} from "@/lib/types";
import { tSwitch } from "@/lib/switchMessages";

type UiPhase = "idle" | "running" | "done" | "error";

const CRED_PREFIX = "cred://v1/account/";

function hasAccountCredential(
  accountId: string,
  meta: CredentialMetaItem[],
): boolean {
  return meta.some(
    (m) => m.account_id === accountId && m.credential_ref.startsWith(CRED_PREFIX),
  );
}

/** 合并 switch_list / account_list 与 credential_meta，导入后的 meta 驱动选择器 */
function mergeAccountsWithMeta(
  list: Account[],
  meta: CredentialMetaItem[],
): Account[] {
  const byId = new Map<string, Account>();
  for (const a of list) byId.set(a.id, a);
  for (const m of meta) {
    if (!m.credential_ref.startsWith(CRED_PREFIX)) continue;
    if (byId.has(m.account_id)) continue;
    byId.set(m.account_id, {
      id: m.account_id,
      platform: "workbuddy",
      displayName: m.label || m.account_id,
      status: "active",
      tags: ["imported"],
    });
  }
  return [...byId.values()];
}

function committedAccountId(evt: SwitchCommittedEvent): string {
  return evt.accountId ?? evt.account_id ?? "";
}

function committedSnapshotId(evt: SwitchCommittedEvent): string {
  return evt.snapshotId ?? evt.snapshot_id ?? "";
}

export function SwitcherPage() {
  const [searchParams] = useSearchParams();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [credMeta, setCredMeta] = useState<CredentialMetaItem[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [prechecking, setPrechecking] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [precheck, setPrecheck] = useState<SwitchPrecheckResult | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [uiPhase, setUiPhase] = useState<UiPhase>("idle");
  const [progress, setProgress] = useState<SwitchProgressEvent | null>(null);
  const [resultKey, setResultKey] = useState<SwitchMessageKey | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rollbackBusy, setRollbackBusy] = useState(false);
  const [lastCommitted, setLastCommitted] = useState<SwitchCommittedEvent | null>(
    null,
  );

  const loadTargets = useCallback(async () => {
    setError(null);
    try {
      // 优先真 IPC：switch_list → account_list；再并 credential_meta
      let list: Account[] = [];
      try {
        list = await switchList();
      } catch {
        list = [];
      }
      if (!list?.length) {
        try {
          list = await accountList();
        } catch {
          list = [];
        }
      }
      let meta: CredentialMetaItem[] = [];
      try {
        const r = await listCredentialMeta();
        meta = r.items ?? [];
      } catch {
        meta = [];
      }
      const merged = mergeAccountsWithMeta(list ?? [], meta);
      setAccounts(merged);
      setCredMeta(meta);
      const fromQuery = searchParams.get("accountId");
      setSelected((prev) => {
        if (fromQuery && merged.some((a) => a.id === fromQuery)) return fromQuery;
        if (prev && merged.some((a) => a.id === prev)) return prev;
        return merged[0]?.id ?? "";
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [searchParams]);

  useEffect(() => {
    void loadTargets();
  }, [loadTargets]);

  // switch://progress
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void listen<SwitchProgressEvent>(EVENTS.switchProgress, (evt) => {
      if (!alive) return;
      if (jobId && evt.jobId !== jobId) return;
      setProgress(evt);
      if (evt.phase === "done") {
        setUiPhase("done");
        setResultKey(evt.messageKey ?? "success");
        setConfirming(false);
        setConfirmOpen(false);
      } else {
        setUiPhase("running");
      }
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [jobId]);

  // switch://committed：accountId / product / snapshotId（兼容 snake_case）
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void listen<SwitchCommittedEvent>(EVENTS.switchCommitted, (evt) => {
      if (!alive) return;
      setLastCommitted(evt);
      // 提交成功后刷新目标列表（导入/绑定可能已变）
      void loadTargets();
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [loadTargets]);

  const selectedMissingCred = useMemo(
    () => (selected ? !hasAccountCredential(selected, credMeta) : false),
    [selected, credMeta],
  );

  async function onPrecheck() {
    if (!selected) return;
    setPrechecking(true);
    setError(null);
    setResultKey(null);
    setProgress(null);
    setLastCommitted(null);
    setUiPhase("idle");
    try {
      const r = await switchPrecheck(selected);
      // 本地 meta 无 cred://v1/account/ 时补 warnings，确保 UI 展示文案
      if (!hasAccountCredential(selected, credMeta)) {
        const warnings: SwitchMessageKey[] = r.warnings.includes(
          "target_credential_missing",
        )
          ? r.warnings
          : [...r.warnings, "target_credential_missing"];
        setPrecheck({ ...r, warnings });
      } else {
        setPrecheck(r);
      }
      setConfirmOpen(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg === "target_credential_missing" || msg.includes("target_credential_missing")) {
        const acc = accounts.find((a) => a.id === selected);
        setPrecheck({
          targetAccountId: selected,
          targetDisplayName: acc?.displayName ?? selected,
          provider: acc?.platform ?? "workbuddy",
          clientRunning: false,
          canProceed: false,
          warnings: ["target_credential_missing"],
          messageKey: "target_credential_missing",
          currentAccountId: null,
        });
        setConfirmOpen(true);
      } else {
        setError(msg);
        setUiPhase("error");
      }
    } finally {
      setPrechecking(false);
    }
  }

  async function onConfirm() {
    if (!selected || !precheck) return;
    if (precheck.clientRunning || !precheck.canProceed) return;
    if (precheck.messageKey === "target_credential_missing") return;
    setConfirming(true);
    setError(null);
    try {
      const r = await switchConfirmExec(selected);
      setJobId(r.jobId);
      setUiPhase("running");
      setProgress({
        jobId: r.jobId,
        phase: "backup",
        percent: 5,
        messageKey: "in_progress",
      });
    } catch (e) {
      setConfirming(false);
      const msg = e instanceof Error ? e.message : String(e);
      if (msg === "target_credential_missing" || msg.includes("target_credential_missing")) {
        setResultKey("target_credential_missing");
        setUiPhase("error");
        setError(tSwitch("target_credential_missing"));
      } else {
        setError(msg);
        setUiPhase("error");
      }
    }
  }

  async function onRollbackRetry() {
    if (!jobId) return;
    setRollbackBusy(true);
    try {
      const r = await switchRollbackRetry(jobId);
      setResultKey(r.messageKey);
      setUiPhase("done");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRollbackBusy(false);
    }
  }

  function onCancelConfirm() {
    if (confirming) return;
    setConfirmOpen(false);
    setPrecheck(null);
  }

  const resultVariant =
    resultKey === "success"
      ? "success"
      : resultKey === "rolled_back"
        ? "warning"
        : resultKey
          ? "danger"
          : "default";

  return (
    <div>
      <PageHeader
        title="换号"
        description="只切换登录态，不迁移 Chat / 工作区。先预检，确认后再执行。"
      />

      <div className="max-w-lg space-y-4">
        <div className="space-y-4 rounded-lg border border-border bg-bg-elevated p-4">
          <label className="block text-xs text-fg-subtle">目标账号</label>
          <select
            className="w-full rounded-md border border-border bg-bg px-3 py-2 text-sm text-fg"
            value={selected}
            onChange={(e) => setSelected(e.target.value)}
            disabled={uiPhase === "running" || confirming}
          >
            {accounts.map((a) => {
              const ok = hasAccountCredential(a.id, credMeta);
              return (
                <option key={a.id} value={a.id}>
                  {a.displayName}（{a.platform === "workbuddy" ? "WorkBuddy" : "TraeCode CN"}）
                  {ok ? "" : " · 缺凭据"}
                  {a.id === "acc-wb-2" ? " · mock:客户端运行中" : ""}
                </option>
              );
            })}
          </select>

          {selectedMissingCred ? (
            <div className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {tSwitch("target_credential_missing")}
            </div>
          ) : null}

          <div className="flex flex-wrap gap-2">
            <Button
              disabled={prechecking || !selected || uiPhase === "running"}
              onClick={() => void onPrecheck()}
            >
              {prechecking ? "预检中…" : "预检并换号"}
            </Button>
            <Button
              variant="ghost"
              disabled={uiPhase === "running"}
              onClick={() => {
                setUiPhase("idle");
                setProgress(null);
                setResultKey(null);
                setError(null);
                setJobId(null);
                setLastCommitted(null);
                void loadTargets();
              }}
            >
              重置
            </Button>
          </div>

          <p className="text-xs text-fg-subtle">
            换号目标用 account_id，且需绑定{" "}
            <code className="text-fg-muted">{CRED_PREFIX}...</code>
            。调试：URL 加 <code className="text-fg-muted">?forceRunning=1</code> 可强制
            client_running。
          </p>
        </div>

        <SwitchProgress
          phase={uiPhase}
          progress={progress}
          resultKey={resultKey}
          error={error}
        />

        {lastCommitted ? (
          <div className="rounded-lg border border-success/40 bg-success/10 px-4 py-3 text-xs text-fg">
            <span className="font-medium text-success">已提交 </span>
            accountId={committedAccountId(lastCommitted)} · product=
            {lastCommitted.product} · snapshotId=
            {committedSnapshotId(lastCommitted)}
          </div>
        ) : null}

        {uiPhase === "done" && resultKey ? (
          <div className="flex flex-wrap items-center gap-3 rounded-lg border border-border bg-bg-elevated p-4 text-sm">
            <Badge variant={resultVariant}>{tSwitch(resultKey)}</Badge>
            {resultKey === "rollback_failed" ? (
              <Button
                size="sm"
                variant="secondary"
                disabled={rollbackBusy || !jobId}
                onClick={() => void onRollbackRetry()}
              >
                {rollbackBusy ? "回滚中…" : "重试回滚"}
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>

      <ConfirmSwitchDialog
        open={confirmOpen}
        precheck={precheck}
        confirming={confirming}
        onCancel={onCancelConfirm}
        onConfirm={() => void onConfirm()}
      />
    </div>
  );
}
