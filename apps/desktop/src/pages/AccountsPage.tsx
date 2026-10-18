import { useEffect, useState } from "react";
import { PageHeader } from "@/components/layout/PageHeader";
import { accountList, listCredentialMeta } from "@/lib/api";
import type { Account, CredentialMetaItem } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

export function AccountsPage() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [credMeta, setCredMeta] = useState<CredentialMetaItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [metaError, setMetaError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setMetaError(null);
      try {
        const [list, meta] = await Promise.all([
          accountList(),
          listCredentialMeta().catch((e: unknown) => {
            setMetaError(e instanceof Error ? e.message : "凭据 meta 加载失败");
            return { items: [] as CredentialMetaItem[] };
          }),
        ]);
        if (cancelled) return;
        setAccounts(list);
        setCredMeta(meta.items ?? []);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const metaByAccount = new Map(credMeta.map((m) => [m.account_id, m]));

  return (
    <div className="space-y-6">
      <PageHeader
        title="账号"
        description="账号列表 / 启用禁用 / 绑定凭据（界面只展示 credential_ref，不展示明文）"
        actions={
          <Button variant="secondary" size="sm" disabled>
            导入账号（后续）
          </Button>
        }
      />

      <div className="overflow-hidden rounded-lg border border-border">
        <table className="w-full text-left text-sm">
          <thead className="bg-bg-elevated text-xs text-fg-subtle">
            <tr>
              <th className="px-4 py-2 font-medium">名称</th>
              <th className="px-4 py-2 font-medium">平台</th>
              <th className="px-4 py-2 font-medium">状态</th>
              <th className="px-4 py-2 font-medium">积分</th>
              <th className="px-4 py-2 font-medium">连签</th>
              <th className="px-4 py-2 font-medium">凭据 ref</th>
              <th className="px-4 py-2 font-medium">标签</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={7} className="px-4 py-8 text-center text-fg-muted">
                  加载中…
                </td>
              </tr>
            ) : accounts.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-4 py-8 text-center text-fg-muted">
                  暂无账号
                </td>
              </tr>
            ) : (
              accounts.map((a) => {
                const meta = metaByAccount.get(a.id);
                return (
                  <tr key={a.id} className="border-t border-border hover:bg-bg-hover/50">
                    <td className="px-4 py-3 font-medium text-fg">{a.displayName}</td>
                    <td className="px-4 py-3 text-fg-muted">
                      {a.platform === "workbuddy" ? "WorkBuddy" : "TraeCode CN"}
                    </td>
                    <td className="px-4 py-3">
                      <StatusBadge status={a.status} />
                    </td>
                    <td className="px-4 py-3 tabular-nums text-fg-muted">
                      {a.points?.balance ?? "—"}
                    </td>
                    <td className="px-4 py-3 tabular-nums text-fg-muted">
                      {a.points?.streakDays ?? "—"}
                    </td>
                    <td className="max-w-[14rem] truncate px-4 py-3 font-mono text-xs text-fg-subtle" title={meta?.credential_ref}>
                      {meta?.credential_ref ?? "—"}
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex flex-wrap gap-1">
                        {a.tags.map((t) => (
                          <Badge key={t} variant="outline">
                            {t}
                          </Badge>
                        ))}
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <section className="rounded-lg border border-border bg-bg-elevated p-4">
        <h2 className="text-sm font-medium text-fg">凭据 meta（credential_meta_store）</h2>
        <p className="mt-1 text-xs text-fg-subtle">
          仅 label / account_id / credential_ref；无 token / cipher。真导入走 storeCredential。
        </p>
        {metaError ? (
          <p className="mt-2 text-sm text-danger">{metaError}</p>
        ) : null}
        <ul className="mt-3 space-y-2 text-sm">
          {credMeta.length === 0 ? (
            <li className="text-fg-muted">暂无绑定 meta</li>
          ) : (
            credMeta.map((m) => (
              <li
                key={m.credential_ref}
                className="flex flex-wrap items-baseline justify-between gap-2 border-b border-border/60 pb-2 last:border-0"
              >
                <span className="text-fg">{m.label}</span>
                <span className="font-mono text-xs text-fg-subtle">{m.credential_ref}</span>
              </li>
            ))
          )}
        </ul>
      </section>
    </div>
  );
}

function StatusBadge({ status }: { status: Account["status"] }) {
  if (status === "active") return <Badge variant="success">启用</Badge>;
  if (status === "expired") return <Badge variant="warning">需重登</Badge>;
  if (status === "error") return <Badge variant="danger">异常</Badge>;
  return <Badge variant="default">停用</Badge>;
}
