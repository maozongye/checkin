# 02 · CheckinOrchestrator

> v1 编排范围：**仅 daily-checkin**（不含旅行/补登）。 合同

> 编排层只负责：选号 → 调 Adapter → 写表 → 发事件。  
> 换号归 **Switcher**；持续监控归 **Monitor**。本模块不越界。

---

## 1. 职责边界

| 模块 | 做 | 不做 |
|------|----|------|
| **CheckinOrchestrator** | 一键/定时签到；读分；health；进度事件；写 `checkin_run` / `checkin_item` | 换号、扫码登录、持久监控、改代理全局态 |
| **Switcher** | 账号切换、凭据落盘/加密、导出 `credentials_ref` | 调签到 API |
| **Monitor** | 长驻探活、积分异动告警、会话失效监听 | 批量签到主路径（可订阅 `checkin:done`） |

---

## 2. 公共 API

```rust
pub struct CheckinOrchestrator {
    adapters: HashMap<String, Arc<dyn PlatformAdapter>>,
    store: Arc<dyn CheckinStore>,          // 表读写
    emitter: Arc<dyn EventEmitter>,        // Tauri emit 抽象
    config: OrchestratorConfig,
}

#[derive(Clone, Debug)]
pub struct OrchestratorConfig {
    /// 默认 3。
    pub concurrency: usize,
    /// 账号间随机延迟上限（毫秒）；0 = 关闭。建议 200..=2000。
    pub jitter_ms: u64,
    /// 默认 `"Asia/Shanghai"`。
    pub timezone: String,
    /// 单账号因 RateLimited / Network 的最大额外重试次数（不含首次）。默认 2。
    pub max_retries: u32,
    /// 是否在本地已有 Success/AlreadyDone 时仍打平台。默认 false。
    pub force_remote: bool,
}

#[derive(Clone, Debug)]
pub enum Trigger {
    Manual,
    Schedule {
        /// cron 表达式或 `HH:MM`（与桌面定时器约定）；仅元数据，真正调度可由外部 cron/daemon 触发。
        cron_or_at: String,
    },
}

impl CheckinOrchestrator {
    /// 对当前启用账号执行一轮签到。
    /// `platform_filter`: `None` | `Some("workbuddy")` | `Some("trae")`。
    pub async fn run_all(
        &self,
        trigger: Trigger,
        platform_filter: Option<&str>,
        dry_run: bool,
    ) -> Result<RunSummary, OrchestratorError>;

    /// 仅 health + fetch_points，不签到（可选，桌面「刷新积分」按钮）。
    pub async fn probe_all(
        &self,
        platform_filter: Option<&str>,
    ) -> Result<Vec<ProbeItem>, OrchestratorError>;
}
```

---

## 3. run_all 流程（写死）

```
1. started_at = now()
2. local_day  = calendar_date(now, config.timezone)  // YYYY-MM-DD
3. accounts   = Switcher.list_enabled(platform_filter)
4. INSERT checkin_run { status: running, trigger, timezone, concurrency, ... }
5. 信号量限制 concurrency（默认 3）
6. 对每个 account（可并行）:
     a. 若 jitter_ms > 0：sleep(random 0..=jitter_ms)
     b. emit checkin:progress { status: "running", ... }
     c. 本地幂等查询 (account_id, local_day[, platform])
        - 已有 Success|AlreadyDone 且 !force_remote → AlreadyDone，跳过远程
     d. 构造 CheckinCtx { run_id, local_day, attempt=1, ... }
     e. adapter.checkin(ctx)
        - RateLimited / Network 且 retryable：退避后 attempt++ 再试，直至 max_retries
        - AuthExpired：允许 Adapter 内 refresh 一次；仍失败则 Failed/Disabled
     f. UPSERT checkin_item
     g. emit checkin:progress { status: outcome.kind, message, ... }
7. 任意单账号失败不阻断其余账号
8. 汇总 counts → status: done | partial | failed
9. UPDATE checkin_run.finished_at / status
10. emit checkin:done { ... }
11. return RunSummary
```

### Run 状态机

| `checkin_run.status` | 条件 |
|----------------------|------|
| `running` | 已创建未结束 |
| `done` | 全部为 Success / AlreadyDone / Skipped（无 Failed/Disabled 或 Disabled 视为需人工但 run 仍可 `partial`——见下） |
| `partial` | 至少一成功类 + 至少一 Failed 或 Disabled |
| `failed` | 全部为 Failed（或无账号可跑） |

建议：`Disabled` 计入 `partial` 而非整 run `failed`，以便定时任务次日继续跑其他号。

---

## 4. 并发与抖动

```rust
// 伪代码
let sem = Arc::new(Semaphore::new(self.config.concurrency)); // default 3
for (index, acc) in accounts.iter().enumerate() {
    let permit = sem.clone().acquire_owned().await?;
    let jitter = if self.config.jitter_ms > 0 {
        rand::thread_rng().gen_range(0..=self.config.jitter_ms)
    } else { 0 };
    tokio::spawn(async move {
        if jitter > 0 { tokio::time::sleep(Duration::from_millis(jitter)).await; }
        // ... checkin ...
        drop(permit);
    });
}
```

- **默认 `concurrency = 3` + jitter**：降低 Trae 同 IP `9074` 概率；Architect 代定多号同出口 concurrency≤3（Q9 已收口）。
- **jitter**：账号启动错峰；与 Adapter 内 9074 退避互补。

---

## 5. 进度事件（Tauri emit）

事件名写死，payload 字段可扩展但下列为最低集。

### `checkin:progress`

```json
{
  "run_id": "uuid",
  "account_id": "wb-001",
  "platform": "workbuddy",
  "index": 0,
  "total": 5,
  "status": "running|Success|AlreadyDone|Skipped|Failed|Disabled",
  "message": "人话摘要，无 token"
}
```

- 每个账号至少两次：进入 `running`、结束时终态。
- `index` 为本 run 账号列表下标（0-based）；`total` 为本 run 账号数。

### `checkin:done`

```json
{
  "run_id": "uuid",
  "trigger": "manual|schedule",
  "cron_or_at": "09:10",
  "started_at": "2026-10-18T09:10:01+08:00",
  "finished_at": "2026-10-18T09:10:08+08:00",
  "timezone": "Asia/Shanghai",
  "local_day": "2026-10-18",
  "counts": {
    "total": 5,
    "success": 2,
    "already_done": 1,
    "skipped": 0,
    "failed": 1,
    "disabled": 1
  },
  "items": [
    {
      "account_id": "wb-001",
      "platform": "workbuddy",
      "outcome": "Success",
      "credits_delta": 10,
      "streak_days": 3,
      "balance": 120,
      "message": "签到成功",
      "raw_code": 0
    }
  ]
}
```

- `items[]` **禁止**含 accessToken / refreshToken / Cookie。
- 时间一律带偏移的 ISO-8601（用户区 Asia/Shanghai）。

---

## 6. RunSummary（返回值）

```rust
#[derive(Clone, Debug, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub trigger: String,          // "manual" | "schedule"
    pub local_day: String,
    pub timezone: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,           // done | partial | failed
    pub counts: RunCounts,
    pub items: Vec<CheckinItemView>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct RunCounts {
    pub total: u32,
    pub success: u32,
    pub already_done: u32,
    pub skipped: u32,
    pub failed: u32,
    pub disabled: u32,
}
```

日积分汇总：`sum(credits_delta) where outcome == Success`（**排除** AlreadyDone）。

---

## 7. Tauri Command 集成点

```rust
#[tauri::command]
async fn checkin_run_all(
    trigger: String,              // "manual" | "schedule"
    cron_or_at: Option<String>,
    platform: Option<String>,     // None = all
    dry_run: Option<bool>,
    state: State<'_, AppState>,
) -> Result<RunSummary, String> {
    let trigger = match trigger.as_str() {
        "schedule" => Trigger::Schedule {
            cron_or_at: cron_or_at.unwrap_or_default(),
        },
        _ => Trigger::Manual,
    };
    state.orchestrator
        .run_all(trigger, platform.as_deref(), dry_run.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}
```

前端订阅：`listen('checkin:progress')` / `listen('checkin:done')`。

---

## 8. 与独立脚本的关系

| | 桌面 Orchestrator | Python `runner.run_all` |
|--|-------------------|-------------------------|
| 并发 | async + semaphore | 当前串行（对照用，可后补线程池） |
| 事件 | Tauri emit | JSON 日志 / 通知 webhook |
| 幂等 | SQLite 表 | 平台业务码 + 可选本地日志 |
| 定位 | 生产路径 | 无桌面 CI / 对照 / 服务器 cron |

合同以桌面为准；脚本字段名尽量对齐（`account_id`、`credits_delta`、`raw_code`）。
