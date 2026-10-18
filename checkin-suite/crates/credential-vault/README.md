# credential-vault

本地凭据仓（WorkBuddy / Trae CN）：OS 钥匙串抽象 + `credential_ref` + 加密备份 + **CredMan blob>2560 混合存储**。

对齐设计合同：`/workspace/docs/credential-module-design-contract-v0.2.md`（v0.2.2）  
溢出补丁：`/workspace/docs/credential-blob-overflow-patch-v1.md`

## 用途

- UI 一次导入 `secret_json` → `CredentialStore::store` → 返回 `cred://v1/account/...`
- Checkin / Adapter 内部 `resolve(ref) → CredentialHandle` → `as_workbuddy_tokens()` / `as_trae_cn_blob()`
- 删除前强制 Argon2id + XChaCha20-Poly1305 加密备份（`.cred.bak`）
- Switcher 换号备份前缀：`cred://v1/switch_backup/{product}/{snapshot_id}`（同 Backend，不进 `account.credential_ref`）

## 混合存储（blob >2560）

CredMan 上限 2560。Backend **只存小指针** `wbtcred_ptr_v1`：

| mode | Backend 内容 | 适用 |
|------|--------------|------|
| `inline` | base64(envelope seal) | seal ≤ `INLINE_MAX`(1800) 且非 Trae |
| `file` | `rel_path` + `file_key_b64`(32B) | **Trae 强制**；或 seal > INLINE_MAX |

文件：`{vault_dir}/blobs/{uuid}.cve` = magic `WBTBLOB1` + nonce(24) + XChaCha20-Poly1305。密钥仅在 Backend。

```rust
CredentialStore::new(backend)                      // 仅 inline；Trae/超大 → 错误
CredentialStore::with_vault_dir(backend, vault_dir) // 生产 / file mode
```

旧「裸 envelope bytes」仍可 resolve（一轮兼容）。

## 非目标

- 签到 / 换号业务逻辑
- 解密 Trae `iCubeAuthInfo` 密文
- 云同步
- **P1：不回写官方 `.info` / `storage.json`**
- `on_switch_committed` upsert（归 Account）
- 分片进多条 CredMan

## Backend

| Backend | 状态 |
|---------|------|
| `MemoryBackend` | ✅ 完整（测试 / dry-run / 非 Windows） |
| **Windows Credential Manager** | ✅ `OsKeychainBackend`（指针 JSON ≪2560） |
| macOS Keychain | ⏳ 后置（非 Windows 仍为 Unavailable stub） |
| Linux 真钥匙串 | ❌ 本期不做 |

钥匙串物理 TargetName / key：`com.benchforge.wb-trae.credential/` + `cred://` 之后的路径。

## 测试（Linux / CI）

```bash
cd crates/credential-vault && cargo test
# windows_credman 在非 Windows 上 cfg 掉（0 tests）
```

关键测例：`small_workbuddy_stores_inline_and_resolves`、`large_trae_file_mode_resolve_and_delete_cleans_cve`、`trae_without_vault_dir_fails`、`legacy_raw_envelope_still_resolves`。

## IPC 草案

见 `/workspace/docs/credential-tauri-ipc-draft.md`（`credential_store` / `credential_delete`；**不**暴露 resolve）。生产构造请用 `with_vault_dir`。

## 给 checkin-core 用

```rust
let vault = CredentialStore::with_vault_dir(os_backend(), app_data_dir.join("vault"));
let handle = vault.resolve_str(&ctx.credentials_ref.key)?;
match platform {
  "workbuddy" => { let t = handle.as_workbuddy_tokens()?; /* → Session */ }
  "trae" | "trae_cn" => { let b = handle.as_trae_cn_blob()?; /* → Session */ }
}
```
