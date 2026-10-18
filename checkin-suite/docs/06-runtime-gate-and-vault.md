# 06 · Runtime 门控 + credential-vault 对接

> 日期：2026-10-18（Asia/Shanghai）  
> 约束：**禁止**日志/事件打印 token；**禁止**平行 VaultBackend。

---

## 1. HTTP 门控（默认关）

```rust
pub enum RuntimeMode { DryRun, LiveHttp }  // default DryRun

pub struct CheckinRuntimeOptions {
    pub mode: RuntimeMode,        // default DryRun
    pub allow_live_http: bool,    // default false
}
```

合并规则（`options.effective(dry_run)`）：

| dry_run 参数 | mode | allow_live_http | 结果 | note |
|--------------|------|-----------------|------|------|
| true | * | * | Dry | `dry_run` |
| false | DryRun | * | Dry | `dry_run` |
| false | LiveHttp | false | **强制 Dry（gated）** | `gated` |
| false | LiveHttp | true | Live | （无 / cron） |

- gated 时 item message 为 `dry_run_gated`。
- **setup 装包前实机验收才开 `allow_live_http=true`**；安装器默认保持关闭。
- LiveHttp 已接 WB + Trae CN 官方 HTTP adapter（`allow_live_http` 默认仍关；关=DryRun）。
- Trae vault 材料可带可选 `access_token`/`refresh_token`；仅 cipher 时 LiveHttp → `trae_jwt_unavailable`（非 stub）。
- 业务码对照 Python；**未**对生产域做 live 验收。

## 2. CredentialsRef ↔ vault

```rust
pub struct CredentialsRef { pub uri: String }  // cred://v1/account/...
```

- `From<&credential_vault::CredentialRef>` / `parse_vault_ref`
- 仅 **account** 前缀可用于签到；`switch_backup` → 拒绝
- 模块：`vault_bridge::resolve_for_checkin(store, cred) -> ResolvedSession`
  - 直接使用已有 `CredentialStore<B>` / `CredentialBackend`（MemoryBackend 可测）
  - `ResolvedSession` Debug **全部 REDACTED**；drop 即 zeroize
- **dry_run / probe**：不 resolve 明文；可 `validate_ref_shape` 或 DesktopPaths 文件存在
- **LiveHttp**：Orchestrator 必须 `vault: Some(...)` 并 resolve；失败 → Disabled/Failed
- DesktopPaths **只作导入入仓来源**，不替代 vault

## 3. Workspace

`/workspace/checkin-suite/Cargo.toml` members：`checkin-core`, `credential-vault`  
`checkin-core` → `credential-vault = { path = "../credential-vault" }`

## 4. 导入入仓

见 [07-import-desktop.md](./07-import-desktop.md)：`import_*` 把本机 `.info` / `storage.json` 写入 `CredentialStore`，返回 `cred://v1/account/...`。DesktopPaths 仅解析路径，不替代 vault。

