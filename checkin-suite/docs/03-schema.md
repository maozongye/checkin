# 03 · 表结构合同（checkin_run / checkin_item）

> 桌面端持久化（建议 SQLite）。脚本侧用 JSON 日志等价，不强制建表。  
> 时间字段存 ISO-8601 带偏移字符串，或 UTC 整数毫秒 + 单独 timezone 列；下文用 TEXT ISO。

---

## 1. checkin_run

一轮编排（手动或定时）一条。

| 列 | 类型 | 约束 | 说明 |
|----|------|------|------|
| `id` | TEXT | PK | UUID / ULID |
| `trigger` | TEXT | NOT NULL | `manual` \| `schedule` |
| `started_at` | TEXT | NOT NULL | ISO-8601 |
| `finished_at` | TEXT | NULL | 结束时写入 |
| `status` | TEXT | NOT NULL | `running` \| `done` \| `partial` \| `failed` |
| `timezone` | TEXT | NOT NULL | 默认 `Asia/Shanghai` |
| `concurrency` | INTEGER | NOT NULL | 本 run 使用的并发度 |
| `note` | TEXT | NULL | 可选：cron_or_at、操作者、dry_run 标记等 |

```sql
CREATE TABLE IF NOT EXISTS checkin_run (
  id           TEXT PRIMARY KEY,
  trigger      TEXT NOT NULL CHECK (trigger IN ('manual', 'schedule')),
  started_at   TEXT NOT NULL,
  finished_at  TEXT,
  status       TEXT NOT NULL CHECK (status IN ('running', 'done', 'partial', 'failed')),
  timezone     TEXT NOT NULL DEFAULT 'Asia/Shanghai',
  concurrency  INTEGER NOT NULL DEFAULT 3,
  note         TEXT
);

CREATE INDEX IF NOT EXISTS idx_checkin_run_started ON checkin_run(started_at DESC);
```

---

## 2. checkin_item

单账号单次结果；**业务幂等**靠唯一约束。

| 列 | 类型 | 约束 | 说明 |
|----|------|------|------|
| `id` | TEXT | PK | UUID |
| `run_id` | TEXT | NOT NULL, FK → checkin_run.id | |
| `account_id` | TEXT | NOT NULL | |
| `platform` | TEXT | NOT NULL | `workbuddy` \| `trae` |
| `local_day` | TEXT | NOT NULL | `YYYY-MM-DD` |
| `outcome` | TEXT | NOT NULL | 见枚举 |
| `credits_delta` | INTEGER | NULL | 本次新增积分 |
| `streak_days` | INTEGER | NULL | 连签天数 |
| `balance` | INTEGER | NULL | 签后余额（若有） |
| `message` | TEXT | NULL | 人话，无密钥 |
| `raw_code` | INTEGER | NULL | 平台业务码 |
| `retryable` | INTEGER | NULL | 0/1；仅 Failed 有意义 |
| `started_at` | TEXT | NOT NULL | |
| `finished_at` | TEXT | NULL | |

### outcome 枚举

`Success` | `AlreadyDone` | `Skipped` | `Failed` | `Disabled`

与 `CheckinOutcome` tag 一致（PascalCase）。

```sql
CREATE TABLE IF NOT EXISTS checkin_item (
  id             TEXT PRIMARY KEY,
  run_id         TEXT NOT NULL REFERENCES checkin_run(id) ON DELETE CASCADE,
  account_id     TEXT NOT NULL,
  platform       TEXT NOT NULL,
  local_day      TEXT NOT NULL,
  outcome        TEXT NOT NULL CHECK (outcome IN (
                   'Success', 'AlreadyDone', 'Skipped', 'Failed', 'Disabled'
                 )),
  credits_delta  INTEGER,
  streak_days    INTEGER,
  balance        INTEGER,
  message        TEXT,
  raw_code       INTEGER,
  retryable      INTEGER,
  started_at     TEXT NOT NULL,
  finished_at    TEXT,
  -- 业务幂等：同一账号同一本地日同一平台至多一条「有效签到记录」
  UNIQUE (account_id, local_day, platform)
);

CREATE INDEX IF NOT EXISTS idx_checkin_item_run ON checkin_item(run_id);
CREATE INDEX IF NOT EXISTS idx_checkin_item_day ON checkin_item(local_day, platform);
```

### 幂等索引说明

- **推荐唯一约束**：`(account_id, local_day, platform)`  
  - 同一自然人可能同时有 WorkBuddy + Trae，必须带 `platform`。  
  - 若产品保证 `account_id` 全局跨平台唯一，可退化成 `(account_id, local_day)`，但 **合同默认带 platform**。
- Orchestrator 在 `force_remote=false` 时：查到已有 `outcome IN ('Success','AlreadyDone')` 则短路返回 `AlreadyDone`，不更新积分列（可更新 `run_id` 指针——**不推荐**；更干净的做法是本 run 插入失败则读旧行映射为 AlreadyDone 事件，不写第二行）。
- 同日 `Failed` 后重跑：允许 **UPDATE** 同一唯一键行（用新 outcome 覆盖），或先删 Failed 再插；实现任选，须在代码注释写明。建议：`ON CONFLICT(account_id, local_day, platform) DO UPDATE`，且当旧值为 `Success`/`AlreadyDone` 时拒绝降级覆盖（除非 `force_remote`）。

---

## 3. AlreadyDone vs Success 记账差异

| | Success | AlreadyDone |
|--|---------|-------------|
| 含义 | 本 run **新签**成功 | 今日早已签（本地或平台 10001/9095） |
| `credits_delta` 写入行 | 平台返回的本次增量 | 可为 NULL；若平台回显历史值可存，**仅作展示** |
| **日汇总积分** | **累加** | **不累加** |
| 通知文案 | 「+N 积分」 | 「已签跳过」 |
| 幂等 | 建立唯一键占位 | 命中唯一键或平台已签码 |

```sql
-- 某日真实获得积分（示例）
SELECT COALESCE(SUM(credits_delta), 0)
FROM checkin_item
WHERE local_day = ? AND outcome = 'Success';
```

---

## 4. 与 Python 对照

| 表/字段 | Python |
|---------|--------|
| `checkin_item.outcome=AlreadyDone` | `CheckinStatus.SKIPPED` + raw_code 10001/9095 |
| `checkin_item.outcome=Skipped` | `SKIPPED`（Global）或 `DRY_RUN` |
| `credits_delta` / `streak_days` / `balance` / `raw_code` | `CheckinResult` 同名字段 |
| `checkin_run` | `RunSummary.started_at/finished_at` + 日志文件 |

---

## 5. 脱敏

- 表中 **禁止** 存 accessToken、refreshToken、Cookie、完整 Authorization。  
- `message` / 日志仅允许 uid 后四位、account_id、业务码。
