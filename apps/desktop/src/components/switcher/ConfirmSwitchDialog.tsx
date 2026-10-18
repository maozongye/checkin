import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import type { SwitchPrecheckResult } from "@/lib/types";
import { tSwitch } from "@/lib/switchMessages";
import { cn } from "@/lib/utils";

export interface ConfirmSwitchDialogProps {
  open: boolean;
  precheck: SwitchPrecheckResult | null;
  confirming: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

function providerLabel(p: SwitchPrecheckResult["provider"]): string {
  return p === "workbuddy" ? "WorkBuddy" : "TraeCode CN";
}

/**
 * ConfirmUI：必须展示「只切登录态，不迁 Chat/工作区」；
 * clientRunning 时禁用主按钮。
 */
export function ConfirmSwitchDialog({
  open,
  precheck,
  confirming,
  onCancel,
  onConfirm,
}: ConfirmSwitchDialogProps) {
  if (!open || !precheck) return null;

  const blockedByClient = precheck.clientRunning;
  const blockedByCred =
    precheck.messageKey === "target_credential_missing" ||
    precheck.warnings.includes("target_credential_missing");
  const blockedByRelogin =
    precheck.messageKey === "need_relogin" ||
    (!precheck.canProceed && !blockedByClient && !blockedByCred);
  const canExec = !blockedByClient && precheck.canProceed && !confirming;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-switch-title"
    >
      <div className="w-full max-w-md rounded-lg border border-border bg-bg-elevated shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 id="confirm-switch-title" className="text-base font-semibold text-fg">
            确认换号
          </h2>
          <p className="mt-1 text-xs text-fg-muted">Buddy 加油站 · 登录态切换</p>
        </div>

        <div className="space-y-3 px-4 py-4 text-sm">
          <div className="rounded-md border border-border bg-bg px-3 py-2">
            <div className="text-xs text-fg-subtle">目标账号</div>
            <div className="mt-0.5 font-medium text-fg">{precheck.targetDisplayName}</div>
            <div className="mt-1 text-xs text-fg-muted">{providerLabel(precheck.provider)}</div>
          </div>

          {/* 合同强制文案 */}
          <div className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs leading-relaxed text-fg">
            <span className="font-medium text-warning">重要：</span>
            只切换登录态，不迁移 Chat / 工作区。本地对话记录与工程目录不会随账号一起搬迁。
          </div>

          {precheck.warnings.length > 0 ? (
            <ul className="space-y-1">
              {precheck.warnings.map((w) => (
                <li key={w}>
                  <Badge variant="default" className="whitespace-normal text-left">
                    {tSwitch(w)}
                  </Badge>
                </li>
              ))}
            </ul>
          ) : null}

          {blockedByClient ? (
            <div className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {tSwitch("client_running")}
            </div>
          ) : null}

          {!blockedByClient && blockedByCred ? (
            <div className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {tSwitch("target_credential_missing")}
            </div>
          ) : null}

          {!blockedByClient && !blockedByCred && blockedByRelogin ? (
            <div className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {tSwitch(precheck.messageKey ?? "need_relogin")}
            </div>
          ) : null}

          {!blockedByClient && precheck.canProceed ? (
            <p className="text-xs text-fg-muted">{tSwitch("precheck_ok")}</p>
          ) : null}
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="ghost" onClick={onCancel} disabled={confirming}>
            取消
          </Button>
          <Button
            disabled={!canExec}
            onClick={onConfirm}
            className={cn(blockedByClient && "opacity-50")}
            title={blockedByClient ? tSwitch("client_running") : undefined}
          >
            {confirming ? "执行中…" : "确认执行"}
          </Button>
        </div>
      </div>
    </div>
  );
}
