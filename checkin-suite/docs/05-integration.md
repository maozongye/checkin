# 05 · 集成说明（独立脚本 vs 桌面模块）

> **v1**：WorkBuddy 桌面 + Trae CN；Windows 优先；仅 daily-checkin；凭据优先本机已验证路径托管。

---

## 1. 双轨定位

| 轨 | 路径 | 角色 |
|----|------|------|
| **独立脚本** | `/workspace/checkin-suite/scripts/` + `checkin/` | 无桌面环境 CI、服务器 cron、对照实测、快速排障 |
| **桌面模块** | Tauri + Rust 同进程 | **生产路径**：PlatformAdapter 真实现、表持久化、Tauri 事件、与 Switcher 凭据打通 |

Python **不是**桌面运行时依赖；仅作对照与无桌面 CI。字段对齐见 01 / 03。

---

## 2. 独立脚本入口

| 脚本 | 场景 | 说明 |
|------|------|------|
| `scripts/checkin_once.py` | **手动触发** | `--platform all\|workbuddy\|trae`；`--dry-run`；`--json`；`--no-notify` |
| `scripts/checkin_daemon.py` | **自动定时（进程内）** | `--at HH:MM`（Asia/Shanghai）循环 |
| `scripts/install_cron.sh` | **自动定时（系统）** | Linux/macOS crontab |
| `scripts/install_schtasks.ps1` | **自动定时（系统）** | Windows 计划任务 |

配置：

- `accounts.example.json` → 复制为 `accounts.json`（**勿提交真凭据**）
- `config.example.json` → `config.json`（WB base 默认 copilot.tencent.com；路径见 DesktopPaths）

日志：`notify.write_local_log` → `logs/` 下 JSON，等价于桌面 `checkin_run`/`checkin_item` 的弱化形态。

```bash
cd /workspace/checkin-suite
python3 scripts/checkin_once.py --platform all --dry-run --json --no-notify
```

---

## 3. 桌面模块集成点

### 3.1 Rust 侧

1. 实现 `PlatformAdapter`（见 `docs/rust-sketch/adapter.rs`）。  
2. `CheckinOrchestrator::run_all` 挂到 Tauri command：

```rust
// 建议 command 名写死
#[tauri::command]
async fn checkin_run_all(...) -> Result<RunSummary, String>;
```

3. `app.emit("checkin:progress", …)` / `app.emit("checkin:done", …)`（事件名写死，见 02）。  
4. SQLite（或桌面现有 DB）建 `checkin_run` / `checkin_item`（见 03）。

### 3.2 凭据

- **走本机加密存储 / Switcher 落盘**，经 `CredentialsRef` 注入 Adapter。  
- **禁止**把 token 写进聊天、issue、git、Tauri 事件 payload。  
- v1：**优先读本机已验证路径并托管**；允许后续导入。Switcher 负责读盘/解密；签到模块只消费 `credentials_ref`。

#### Switcher 凭据来源（相对路径，无绝对用户路径、无 token）

| 平台 | 环境变量根 | 相对路径（本机 Windows 已验证默认） | 键提示 |
|------|------------|--------------------------------------|--------|
| WorkBuddy | `%LOCALAPPDATA%` | `CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info` | `account.uid`；`auth` / `accessToken`；`domain` |
| Trae CN | `%APPDATA%` | `Trae CN/User/globalStorage/storage.json` | `iCubeAuthInfo://icube.cloudide`；`telemetry.devDeviceId` |

dry_run：`checkin_core::probe` 可实读上述路径，仅返回 `PathProbeSummary`（`has_access_token` / `domain` / `device_id_present` 等），**永不回传 JWT 明文**。

### 3.3 前端

- 按钮「立即签到」→ `invoke('checkin_run_all', { trigger: 'manual' })`。  
- 定时：桌面自带 scheduler 或 OS 任务唤醒后 `trigger: 'schedule', cron_or_at: '09:10'`。  
- UI 订阅 `checkin:progress` / `checkin:done` 刷新列表。

### 3.4 与 Switcher / Monitor

```
Switcher ──credentials_ref──▶ Orchestrator ──emit──▶ UI
                                  │
                                  ├── checkin_run / checkin_item
                                  └── (可选) Monitor 订阅 checkin:done
```

Orchestrator **不**调用换号 API；发现 `Disabled` 时可 emit 额外业务事件（非本合同强制）供 Switcher 标红。

---

## 4. 事件与表：谁持久化

| 数据 | 桌面 | 脚本 |
|------|------|------|
| `checkin_run` / `checkin_item` | DB 必选 | 可选；默认 JSON 日志 |
| `checkin:progress` / `checkin:done` | Tauri emit | stdout / 日志行等价 |
| 通知 | 桌面通知中心 / 可选 webhook | wecom / pushplus / 本地日志 |

---

## 5. 测试策略

| 层 | 做法 |
|----|------|
| Rust 单测 | Adapter 用 mock HTTP；幂等键 / outcome 映射表驱动 |
| Python | 已有 `tests/test_idempotent.py`、`test_runner.py`（fixture，无外网） |
| 联调 | dry_run → 单账号实机（用户授权后）→ 多账号 concurrency=3 |

实机联调前必须完成 HANDOFF Q1–Q10 中与凭据/设备相关的项。

---

## 6. 目录速查

```
checkin-suite/
  checkin/                 # Python 对照实现
  scripts/                 # once / daemon / cron / schtasks
  tests/
  crates/checkin-core/   # Rust desktop stub + probe
  docs/
    01-platform-adapter.md
    02-orchestrator.md
    03-schema.md
    04-platforms.md
    05-integration.md
    rust-sketch/adapter.rs
    HANDOFF-to-Architect.md
```
