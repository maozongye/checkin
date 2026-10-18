# 08 · Auth / Frontend 联调清单（Checkin 侧）

> 日期：2026-10-18（Asia/Shanghai）  
> 约束：**禁止本机打包**；**禁止**开启 `allow_live_http`；报告/IPC **禁止**回传 token / 密文。

---

## 1. Checkin 侧已就绪

| 项 | 位置 / 说明 |
|----|-------------|
| 单平台导入 | `import_workbuddy_*` / `import_trae_*` → `ImportReport` |
| 一键双导入 | `import_both_defaults` / `import_both_files` → `ImportBothReport` |
| 错误脱敏 | `format_import_error`：优先 `StoreError::code()` → 中文建议文案（见 `/workspace/docs/credential-store-error-codes-for-checkin.md`） |
| 凭据句柄 | `CredentialsRef`（`cred://v1/account/...`） |
| 运行时解析 | `vault_bridge::resolve_for_checkin` → `ResolvedSession` |
| 门控 | 默认 `RuntimeMode::DryRun`；`allow_live_http = false`（可安装包本地开关亦默认关） |
| LiveHttp adapter | WB `daily-checkin` + Trae `claim` 已实现（wiremock 绿）；**生产域 live 验收暂不做** |
| Trae 材料 JWT | `TraeCnICubeBlob` **向后兼容**可选 `access_token` / `refresh_token`（见 §2.1）；缺 JWT → LiveHttp `Failed{trae_jwt_unavailable}` |
| CLI | `import_desktop --platform workbuddy\|trae_cn\|both`（MemoryBackend；Trae/`both` 需 `--vault-dir`） |
| Trae 仓 | 必须 `CredentialStore::with_vault_dir`（Auth 外置 `.cve` blob） |

本机路径（`DesktopPaths`）**只作导入源**，不替代 vault。

---

## 2. Auth blob>2560（已落地）

> 依据：`/workspace/docs/credential-blob-overflow-patch-v1.md`（Auth 已实现；credential-vault 测绿）  
> 错误码口径：`/workspace/docs/credential-store-error-codes-for-checkin.md`

| 项 | 现状 |
|----|------|
| 策略 | 混合存储：小 envelope inline；大 payload / **Trae 强制** → `{vault_dir}/blobs/*.cve`（file mode） |
| WB | seal 后 ≤ `INLINE_MAX`（实现 1800）可 inline；否则同 file |
| Trae | **强制 file**；`CredentialStore::new`（无 `vault_dir`）导入 **明确失败** → `vault_dir_required`（`ImportError::VaultDirRequired` 早失败 + `StoreError::VaultDirRequired`） |
| 缺 vault_dir | **非静默成功**；UI/IPC 见 `vault_dir_required: …` |
| `payload_too_large` | **仅残余失败**：`StoreError::PayloadTooLarge`（put_raw 超 soft max / 仍无法溢出落盘）。正常 Trae + `with_vault_dir` **不应**再靠此码当主路径 |
| 枚举 | 以 vault 源码为准：已有 `VaultDirRequired`、`PayloadTooLarge { size, limit }`（无单独 `TooLarge` 名） |
| Checkin 映射 | `format_import_error` **优先** `e.code()` → `{code}: {中文建议}`；禁止回显 secret/token/cipher/file_key |

仍待 Auth/目标机：

1. **Win CredMan Backend**（`OsKeychainBackend`）真机 `store` / `resolve`。  
2. 生产 Tauri 注入 `CredentialStore::with_vault_dir(app_data_dir)`。

---

## 2.1 Trae JWT 材料字段（Checkin 已兼容；写入归 Auth）

> LiveHttp Trae 需要明文 JWT；今日 import 默认仍只写 `icube_auth_info_cipher` + `device_id`。  
> **真写入路径由 Auth 负责**（本清单只约定材料形状与 Checkin 消费语义）。

| 项 | 约定 |
|----|------|
| Kind | 仍为 `MaterialKind::TraeCnICubeBlob`（不新开 kind） |
| 必填 | `icube_auth_info_cipher`、`device_id`（cipher-only 旧数据继续可 store/resolve） |
| 可选（JWT） | `access_token`、`refresh_token`（缺省空 / 省略均可；serde 向后兼容） |
| Resolve | `ResolvedSession::Trae` 带出 device_id + 可选 JWT；`access_token` 空 → `needs_desktop_session` |
| LiveHttp | 有 JWT → `POST {ug}/trae/api/v2/ug/checkin_credits/claim`；无 JWT → `Failed { message: "trae_jwt_unavailable", retryable: false }`（**不是** stub） |
| Auth 待办 | 解密 / 注入路径：在 store 的 secret_json 中写入可选 `access_token`/`refresh_token`（勿经 IPC/日志回传明文） |
| Checkin 不做 | 本阶段不解密 iCube cipher；不改默认 import 只写 cipher；不开生产域 live 验收 |

示例（Auth 写入时可含 JWT；日志/IPC 禁止回显）：

```json
{
  "icube_auth_info_cipher": "<opaque>",
  "device_id": "<telemetry.devDeviceId>",
  "access_token": "<optional Cloud-IDE-JWT payload without logging>",
  "refresh_token": "<optional>"
}
```

---

## 3. 待 Frontend IPC

建议 command（返回仅报告，无 secret）：

| Command | 参数（草案） | 返回 |
|---------|--------------|------|
| `credential_import_desktop` | `{ platform: "workbuddy"\|"trae_cn", path?: string, replace_ref?: string }` | `ImportReport` 或错误码+`format_import_error` |
| `checkin_import_both` | `{ wb_path?: string, trae_path?: string, wb_replace?: string, trae_replace?: string }`（缺省 → DesktopPaths） | `ImportBothReport` |

失败不抛未脱敏 Debug；前端只展示 `*_error` 字符串与 `credential_ref`。

---

## 4. 联调顺序

1. **MemoryBackend 冒烟**：fixture → `import_both_files` → `any_ok` / `reports()` → `resolve_for_checkin`。  
2. **CredMan 小 payload（WB）**：本机 `.info` → `OsKeychainBackend::store` → resolve。  
3. **Trae + `with_vault_dir`**：本机 `storage.json` → **应成功**（file mode）；**非** `payload_too_large`。无 vault_dir → `vault_dir_required`。  
4. **失败路径**：缺文件、EnvUnresolved、单边失败另一边成功、replace_ref 冲突、`payload_too_large` 仅测真实超 soft-max 残余。

Adapter 已非 stub，但联调全程保持 `allow_live_http=false`；**生产域 live 验收暂不做**。

---

## 5. 明确不做（本阶段）

- 本机打包 / setup 安装器联调（用户明确不要）。  
- 开启 `allow_live_http` / 对 `copilot.tencent.com` / `api.trae.cn` 做生产域 live 验收。  
- Trae JWT 真写入（归 Auth；Checkin 只消费可选字段）。  
- 在日志、事件、IPC 中打印 token / iCube 密文。  
- 臆造与 vault 枚举平行的 `TooLarge` 名（以 `PayloadTooLarge` / `code()` 为准）。
