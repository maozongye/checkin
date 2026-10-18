# Monitor v1 — 合同与实现说明

> 路径：`apps/desktop` · 数据：`{app_data}/monitor/monitor.db` · 不存凭据

## 1. 模块布局

```
src-tauri/src/monitor/
  mod.rs        # get_dashboard / refresh
  types.rs      # IPC 类型、枚举、dedupe 辅助
  db.rs         # SQLite 迁移 + MonitorState
  snapshot.rs   # points_snapshot 物化
  health.rs     # session 探活 stub（等 Auth）
  alerts.rs     # 评估 / dedupe / emit monitor:alert
  scheduler.rs  # 间隔与节流（默认 30min / 30s）
commands/monitor.rs  # IPC 薄封装
```

## 2. 表结构

| 表 | 说明 |
|----|------|
| `accounts` | 最小账号元数据（id/platform/display_name/status），**无凭据** |
| `points_snapshot` | 积分缓存（balance/streak_days/expires_at/fetched_at） |
| `monitor_account_state` | 看板物化行（余额/连签/session_*/last_checkin_*） |
| `monitor_alert_config` | 规则开关与阈值 |
| `monitor_settings` | KV：`alerts.enabled`、`health_interval_minutes=30`、`refresh_throttle_secs=30` |
| `monitor_alert_log` | 已触发告警（含 `dedupe_key`） |

### 种子 alert_config

| rule_id | enabled | threshold | cooldown |
|---------|---------|-----------|----------|
| checkin_fail | 1 | — | 24h |
| session_expired | 1 | — | 24h |
| session_expiring | 0 | 48h | 24h |
| points_expiring | 0 | 168h (7d) | 24h |
| points_low | 0 | value=100 | 24h |

空库自动写入与旧 mock 同 id 的 **DEV seed**（`raw_json` 含 `"_dev":true`）。

## 3. IPC

| Command | 行为 |
|---------|------|
| `monitor_get_dashboard` | 纯读 `monitor_account_state` 聚合 summary，不触网 |
| `monitor_refresh` | 节流 30s → 物化 snapshot → `health::check` stub → `alerts::evaluate` → 返回 dashboard |

### Dashboard 字段（camelCase）

- `summary`: `{ healthy, warn, error }`（按 session 合同枚举计数）
- `updatedAt`: unix ms
- `accounts[]`: 余额 / 连签 / `session` / `lastCheckin`
- `alerts?`: 最近告警（来自 `monitor_alert_log`）

### Session 合同枚举

IPC `session.status` **仅**：`ok | expiring | expired | unknown`  
**禁止**再输出 `warn` / `error` / `need_relogin` 作为 session.status。

`lastCheckin.status`：**`ok | fail | none`**（推荐；`success`/`failed` 等入库存时归一化）。

### Summary 计数

| session.status | summary 桶 |
|----------------|------------|
| ok | healthy |
| expiring / unknown | warn |
| expired | error |

## 4. Session 合同 → 旧前端映射表

供 Frontend 色点 / 文案迁移（Architect 转达）：

| 合同枚举 (IPC) | 旧前端 SessionHealthStatus | 色点建议 | 文案建议 |
|----------------|---------------------------|----------|----------|
| `ok` | `ok` | 绿 `bg-success` | 正常 |
| `expiring` | `warn` | 黄 `bg-warning` | 将过期 |
| `expired` | `need_relogin`（亦可视为 `error`） | 红 `bg-danger` | 需重登 |
| `unknown` | `unknown` | 灰 `bg-fg-subtle` | 未知 |

> 后端 **不再** 输出旧值；前端可读兼容旧值，但新数据以合同四态为准。

## 5. 告警

### 事件

- 名：`monitor:alert`
- 载荷（必填）：

```json
{
  "id": "alert-...",
  "rule": "checkin_fail|session_expired|session_expiring|points_expiring|points_low",
  "accountId": "acc-...",
  "severity": "info|warn|error",
  "title": "...",
  "body": "...",
  "at": 1234567890,
  "dedupeKey": "rule:accountId:bucket"
}
```

双写兼容：`level`(=severity)、`message`(=body)。

### severity 映射

| rule | severity |
|------|----------|
| checkin_fail | error |
| session_expired | error |
| session_expiring | warn |
| points_expiring | warn |
| points_low | warn |

### dedupe_key

格式：`{rule}:{account_id}:{bucket}`

| rule | bucket |
|------|--------|
| checkin_fail | 签到日 `YYYY-MM-DD`（Asia/Shanghai；无 at 则用今日） |
| session_expired | 固定 `expired` |
| session_expiring | 会话到期日（Shanghai） |
| points_expiring | 积分到期日（Shanghai） |
| points_low | `lte{threshold_value}` |

同一 `dedupe_key` 在 `cooldown_hours` 内不重复触发。

## 6. health::check stub

- 签名：`check_account(account_id, session_expires_at, threshold_hours) -> SessionStatus`
- 规则：已过期 → `expired`；距到期 < threshold（默认 48h）→ `expiring`；否则 `ok`；无到期数据 → `unknown`
- **不**调 Auth；源码保留 `// TODO: Auth session probe`

探活节奏：settings `health_interval_minutes` 默认 **30**；在 `monitor_refresh` 路径内按间隔执行（无重型常驻定时器）。

## 7. 明确不做（v1）

- OpenAI 兼容额度接口
- 真实 Auth 探活
- Token/积分消耗趋势
- 打包

## 8. 联调提示

1. 启动 App 后 `monitor_get_dashboard` 应看到 4 个 DEV seed 账号  
2. `monitor_refresh` 会评估告警：默认应能看到 `checkin_fail`(acc-wb-2) 与 `session_expired`(acc-trae-2)  
3. 30s 内重复 refresh 被节流，仍返回当前 dashboard  
