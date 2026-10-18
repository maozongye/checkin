# 换号 UI · Switcher 合同接线

App：**Buddy 加油站**  
路径：`/switcher`（支持 `?accountId=` 预选；`?forceRunning=1` 强制 client_running）

## 流程

1. 进入页 → `switch_list`（失败/空则回退 `account_list`）
2. 选目标 →「预检并换号」→ `switch_precheck`
3. 打开 `ConfirmSwitchDialog`（必显：**只切换登录态，不迁移 Chat / 工作区**）
4. `clientRunning === true` → 禁用「确认执行」，展示 `tSwitch("client_running")`
5. 确认 → `switch_confirm_exec({ accountId, product? })` → listen `switch://progress`
6. 成功另有事件 **`switch://committed`**：`{ accountId, product, snapshotId }`（camelCase）
7. 结束按 `message_key`；`rollback_failed` → `switch_rollback_retry`

## 多号目标（Architect 定稿）

- 输入：`account_id` + `product`（`wb_desktop`|`workbuddy`|`trae_cn`）
- 目标明文：账号凭据仓 **`resolve(cred://v1/account/...)`**（不是 switch_backup）
- 流程：R5 硬阻断 → `put_switch_backup` 当前官方态 → Adapter 写**目标** → 失败则 `get_switch_backup_seal` 回滚
- 无目标 account 凭据 / resolve NotFound → **`target_credential_missing`**（不静默、不做 roundtrip）
- Switcher **不**写 `cred://v1/account/...`

## Commands

| Command | 参数 |
|---------|------|
| `switch_precheck` | `accountId`, `forceRunning?`, `product?` |
| `switch_confirm_exec` | `accountId`, `product?` |
| `switch_rollback_retry` | `jobId` |
| `switch_list` | — |

Events：`switch://progress` · `switch://committed`

## Mock

- `SWITCHER_MOCK_SCENARIOS=1` 或 debug：`acc-wb-2` / `acc-trae-2` 跳过真写测 UI；预检不因缺凭据阻断
- Release 真路径：缺 `cred://v1/account/...` meta → `target_credential_missing`

## Rust 模块

`src-tauri/src/switcher/`（adapter / material / backup=Vault 真 API / db / process）+ `commands/switch.rs`。

备份：`{vault_dir}/switch_blobs/{snapshot_id}.cve`；禁 `put_raw` 大包。  
**禁止**本机打包/发版构建。
