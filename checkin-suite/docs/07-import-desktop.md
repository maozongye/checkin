# 07 · 本机登录态导入 → credential-vault

> 日期：2026-10-18（Asia/Shanghai）  
> 约束：本机路径**只作导入源**；**禁止**日志/报告打印 token / 密文；真 HTTP 仍默认关闭。

## API

- `import_workbuddy_info_file(store, path, replace_ref?)` → `ImportReport`
- `import_workbuddy_default_path(store, &DesktopPaths, …)`（`%LOCALAPPDATA%`）
- `import_trae_storage_file(store, path, &DesktopPaths, …)`（`cipher`+`device_id` 必写；可选明文 JWT；**不解密** cipher）
- `import_trae_default_path(store, &DesktopPaths, …)`（`%APPDATA%`）
- `import_both_defaults(store, &DesktopPaths)` / `import_both_files(...)` → `ImportBothReport`（单边失败不阻断）
- `format_import_error`（CredMan 溢出启发式 `payload_too_large`）

`ImportReport`：`platform` / `account_key` / `credential_ref` / `source_path` / `domain?` / `device_id_present` — **无 secret**。

## CLI

```bash
cargo run -p checkin-core --bin import_desktop -- --platform workbuddy --path ./fixture.info
cargo run -p checkin-core --bin import_desktop -- --platform trae_cn --path ./storage.json
cargo run -p checkin-core --bin import_desktop -- --platform both
cargo run -p checkin-core --bin import_desktop -- --platform both --wb-path ./wb.info --trae-path ./storage.json
```

默认 `MemoryBackend`，stdout 打印 `ImportReport` JSON。`--persist-memory-dump` 仅输出条目数（测试用）。

## 验收流

本机 `.info` / `storage.json` → import → `cred://v1/account/...` →（日后）`resolve_for_checkin`；`allow_live_http` 保持 false。

## Trae JWT（可选）

导入尽力从 `storage.json` 提取明文 `access_token`/`accessToken`/`AccessToken`（及对应 refresh 键）；可写顶层或一层嵌套 auth 对象。找不到则行为与旧版一致（仅 cipher+device_id）；resolve 后 access 空 → `needs_desktop_session` → LiveHttp `trae_jwt_unavailable`。**ImportReport 从不回显 token。**

## Trae / oversized

Trae 与超限密文需 `CredentialStore::with_vault_dir`；CLI 对 `trae_cn` / `both` 必填 `--vault-dir`。
