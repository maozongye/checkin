# Buddy 加油站 — `src-tauri`（Tauri 2）

> 路径：`/workspace/apps/desktop/src-tauri`  
> **骨架 + 生产凭据仓已注入；勿在用户本机打包验证。**

## 定稿标识

| 项 | 值 |
|----|-----|
| productName（打包 ASCII） | `BuddyGasStation` |
| identifier | `com.benchforge.buddy-gasstation` |
| 窗口标题 | `Buddy 加油站` |
| version | `0.1.0` |
| bundle.targets | `nsis` + `msi`（Ops 草稿；无签名） |

`tauri.conf.json` **以 Ops** `packaging/tauri.conf.json` 为准（已 `cp` 覆盖）。

## 凭据仓（credential-vault）

| 项 | 值 |
|----|-----|
| crate | `../../../checkin-suite/crates/credential-vault` |
| vault_dir | `{app_data_dir}/credential-vault` |
| blobs | `{vault_dir}/blobs/*.cve` |
| backups | `{vault_dir}/backups/*.cred.bak` |
| Windows backend | `OsKeychainBackend`（CredMan）+ `with_vault_dir` |
| 非 Windows | `MemoryBackend` + `with_vault_dir`（Linux box 可测） |
| app_data 解析 | 优先 `app.path().app_data_dir()`；失败回退 `dirs::data_dir()/buddy-gasstation` |

注入点：`src/lib.rs` → `.setup` → `VaultState`（`src/vault.rs`）。

### Command 表

| Command | 用途 | 备注 |
|---------|------|------|
| `credential_store` | 真导入（secret_json 一次入仓） | 返回 `{ credential_ref, kind, schema_version }` |
| `credential_delete` | 真删除（先加密备份） | 需 `backup_password`；`backup_dir` 可空→`vault_dir/backups` |
| `credential_meta_store` | 仅 meta put/get/delete/list | **无 secret**；壳层 list 过渡用 |
| （禁止）`resolve` | — | 仅同进程 Rust / Adapter |

完整签名见仓库根 `docs/credential-tauri-ipc-draft.md`。

### Frontend 应收尾（React / `lib/api.ts`）

旧壳调用 `credential_store({ action: "put"|"list"|... })` **已失效**。请改：

```ts
// 真导入
await invoke("credential_store", {
  args: {
    platform: "workbuddy" | "trae_cn",
    account_key: string,
    kind: "workbuddy_tokens" | "trae_cn_icube_blob",
    secret_json: string, // 成功后立即丢弃
    source_hint?: string,
    replace_ref?: string,
  },
});

// 真删除
await invoke("credential_delete", {
  args: {
    credential_ref: string,
    backup_password: string,
    backup_dir?: string, // 可省略 → 后端用 vault_dir/backups
  },
});

// 仅 meta（壳 list / 过渡）
await invoke("credential_meta_store", {
  args: {
    action: "list" | "put" | "get" | "delete",
    account_id?: string,
    credential_ref?: string,
    label?: string,
  },
});
```

错误形状：`{ code, message }`（`message` 禁止含 token/密文）。新增码：`vault_dir_required` / `payload_too_large`。

## Icons

已由 `npx @tauri-apps/cli icon packaging/icons/icon-source.png` 生成至 `icons/`。

## 其它已注册 command

`account_list` · `checkin_run_all` · `scheduler_get` / `scheduler_set` · `switch_*`（`switch.rs`，Switcher 维护，**勿改**）· `monitor_get_dashboard` / `monitor_refresh` · `greet`

## 开发 / 构建

在 `apps/desktop`：

```bash
npm install
npm run tauri dev          # beforeDevCommand → npm run dev @ :1420
# 仅类型检查 / 编译（本机 box）：
cd src-tauri && cargo check
# 禁止在用户机：npm run tauri build / NSIS 打包
```

Windows 产物说明见 `../packaging/README-windows.md`。
