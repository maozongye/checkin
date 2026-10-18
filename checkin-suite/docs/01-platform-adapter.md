# 01 · PlatformAdapter 合同

> 面向 Tauri + Rust 同进程桌面端。Python 参考实现见 `checkin/`，字段对齐但以本文 Rust trait / 类型为准。  
> 平台常量（域名、ClientID、桌面路径）一律为 **已知候选 / 未拍板**，以用户确认与桌面端实测为准；合同接口本身可写死。

---

## 1. PlatformAdapter trait

```rust
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// 平台标识，固定小写：`"workbuddy"` | `"trae"`。
    fn platform(&self) -> &'static str;

    /// 执行一次签到（幂等）。同一 `(account_id, local_day)` 二次调用必须返回
    /// `AlreadyDone`（或与平台业务码 10001 / 9095 对齐），不得重复记账。
    async fn checkin(&self, ctx: &CheckinCtx) -> Result<CheckinOutcome, AdapterError>;

    /// 拉取当前积分/余额快照（不触发签到副作用）。
    /// WorkBuddy：可走 checkin-activity-status 或独立余额接口（待确认）。
    /// Trae：可走 status + entitlement（是否拉 entitlement 见 Q10）。
    async fn fetch_points(&self, ctx: &CheckinCtx) -> Result<PointsSnapshot, AdapterError>;

    /// 轻量探活：鉴权是否有效、设备/域名是否可用。不改积分。
    async fn health(&self, ctx: &CheckinCtx) -> Result<HealthReport, AdapterError>;
}
```

### 语义约束

| 方法 | 副作用 | 失败策略 | 幂等 |
|------|--------|----------|------|
| `checkin` | 可能写平台侧「今日已签」 | 返回 `Failed`/`Disabled`，不抛未捕获 panic | 必须 |
| `fetch_points` | 无 | 网络/鉴权错误 → `AdapterError` | 是（只读） |
| `health` | 无 | 同上 | 是（只读） |

- `dry_run == true` 时：`checkin` **不得**发真实签到 POST；可返回 `Skipped { reason: "dry_run" }` 或实现方约定的演练结果（桌面端建议统一 `Skipped`，与 Python `DRY_RUN` 在表映射时记为 `Skipped`）。
- 以 **平台业务码** 判定结果；**禁止**仅凭 `today_checked_in` 预判而跳过签到（该字段已知不可靠，见 04）。

---

## 2. CheckinCtx

```rust
#[derive(Clone, Debug)]
pub struct CheckinCtx {
    /// 账号稳定 ID（桌面端分配或 Switcher 落盘键）。
    pub account_id: String,
    /// `"workbuddy"` | `"trae"`。
    pub platform: String,
    /// 凭据句柄：指向本机加密存储 / Switcher 落盘条目，**禁止**把 token 明文塞进事件或日志。
    pub credentials_ref: CredentialsRef,
    /// 本地业务日，格式 `YYYY-MM-DD`，按 `timezone` 计算。
    pub local_day: String,
    /// IANA 时区；默认 `"Asia/Shanghai"`。
    pub timezone: String,
    /// 演练：不发真实签到。
    pub dry_run: bool,
    /// 本次编排 run 的 UUID / snowflake。
    pub run_id: String,
    /// 本账号在本 run 内的尝试序号，从 1 起（限流重试时递增）。
    pub attempt: u32,
}

/// 凭据引用（具体存储由桌面端实现）。
#[derive(Clone, Debug)]
pub struct CredentialsRef {
    pub store: String,   // 例: "switcher" | "keyring" | "accounts_json"
    pub key: String,     // 例: account_id 或文件相对路径
}
```

### 字段语义

| 字段 | 必填 | 说明 |
|------|------|------|
| `account_id` | 是 | 幂等键组成部分 |
| `platform` | 是 | 选择 Adapter 实现 |
| `credentials_ref` | 是 | Adapter 内 resolve → uid/token/deviceId 等 |
| `local_day` | 是 | Orchestrator 在调度时按 timezone 算好传入 |
| `timezone` | 是 | 默认 `Asia/Shanghai` |
| `dry_run` | 是 | 默认 `false` |
| `run_id` | 是 | 关联 `checkin_run.id` |
| `attempt` | 是 | 首次为 `1` |

---

## 3. CheckinOutcome

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum CheckinOutcome {
    /// 本次新签成功，可记账 credits_delta。
    Success {
        credits_delta: Option<i64>,
        streak_days: Option<i32>,
        balance: Option<i64>,
        message: String,
        raw_code: Option<i64>,
    },
    /// 今日已签（平台码或本地幂等命中）。不累加日汇总积分。
    AlreadyDone {
        credits_delta: Option<i64>, // 通常 None；若平台回显历史值可填但不计入日汇总
        streak_days: Option<i32>,
        balance: Option<i64>,
        message: String,
        raw_code: Option<i64>,      // WorkBuddy 10001 / Trae 9095
    },
    /// 主动跳过：Global 域名、dry_run、账号策略等。
    Skipped {
        reason: String,
    },
    /// 失败；`retryable` 供 Orchestrator 决定是否退避重试。
    Failed {
        code: Option<i64>,
        message: String,
        retryable: bool,
    },
    /// 会话失效 / 账号停用，后续自动 run 应跳过直至人工修复。
    Disabled {
        reason: String,
    },
}
```

### 与 Python CheckinStatus 对照

| Rust Outcome | Python `CheckinStatus` | 表 `checkin_item.outcome` |
|--------------|------------------------|---------------------------|
| `Success` | `success` | `Success` |
| `AlreadyDone` | `skipped`（已签） | `AlreadyDone` |
| `Skipped` | `skipped` / `dry_run` | `Skipped` |
| `Failed` | `failed` | `Failed` |
| `Disabled` | `disabled` | `Disabled` |

> 桌面端表结构把「已签」与「策略跳过」拆开，比 Python 更细；Python 对照脚本可继续把已签记为 `skipped`。

---

## 4. 幂等规则（写死）

1. **唯一键**：`(account_id, local_day)`；若同账号可挂多平台，用 `(account_id, local_day, platform)`（见 03-schema）。
2. **时区**：默认 `Asia/Shanghai`；`local_day` 由 Orchestrator 在调用前按该时区切日。
3. **二次调用**：同一唯一键再次 `checkin`：
   - 本地已有 `Success` / `AlreadyDone` 记录 → 直接返回 `AlreadyDone`，**不打平台**（可选配置 `force_remote=true` 绕过，默认关）。
   - 或打到平台收到业务码 **WorkBuddy `10001`** / **Trae `9095`** → 映射为 `AlreadyDone`。
4. **不得重复记账**：`AlreadyDone` 的 `credits_delta` **不**累加到日汇总 / 通知「今日获得」。
5. **判定优先级**：平台业务码 > HTTP 状态码；HTTP 400 仍解析 body 业务码。禁止仅用 `today_checked_in` 预判漏签。

---

## 5. PointsSnapshot / HealthReport

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointsSnapshot {
    pub balance: Option<i64>,
    pub streak_days: Option<i32>,
    pub today_checked_in: Option<bool>, // 仅展示；不可作为签到跳过依据
    pub raw: serde_json::Value,         // 原始片段，日志需脱敏
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthReport {
    pub ok: bool,
    pub auth_valid: bool,
    pub message: String,
    pub detail: serde_json::Value,      // 例: { "base_url": "...", "device_ok": true }
}
```

---

## 6. AdapterError

```rust
#[derive(Clone, Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("network: {0}")]
    Network(String),
    /// access/refresh 过期，可尝试 refresh 后重试一次。
    #[error("auth expired: {0}")]
    AuthExpired(String),
    /// 凭据无效 / 会话 12153 等，应 Disabled。
    #[error("auth invalid: {0}")]
    AuthInvalid(String),
    /// 限流（如 Trae 9074），retryable。
    #[error("rate limited: {0}")]
    RateLimited(String),
    /// 响应无法解析 / 协议变更。
    #[error("protocol: {0}")]
    Protocol(String),
    /// 缺 deviceId、缺 token、Global 误配等。
    #[error("config: {0}")]
    Config(String),
}
```

### 映射建议

| 场景 | Error / Outcome |
|------|-----------------|
| DNS/超时/5xx | `Network`；Orchestrator 可标 `Failed { retryable: true }` |
| 401/403、业务 1001/1002，refresh 成功后重试 | 内部消化；仍失败 → `AuthExpired` → `Failed` |
| WorkBuddy `12153` | `AuthInvalid` → Outcome `Disabled` |
| Trae `9074` | `RateLimited` → `Failed { retryable: true }` |
| Trae `9004` 假设备 | `Config` → `Failed { retryable: false }` |
| 缺 accessToken / deviceId | `Config` |

Adapter 实现可选择：在 trait 内把部分错误直接转为 `Ok(CheckinOutcome::…)`（推荐对业务可预期码），仅把意外/传输层问题返回 `Err(AdapterError)`。

---

## 7. 实现检查清单（Rust）

- [ ] `platform()` 返回稳定静态字符串  
- [ ] `checkin` 尊重 `dry_run` 与本地幂等键  
- [ ] 业务码 10001/9095 → `AlreadyDone`  
- [ ] token **不**进入 `message` / Tauri 事件 / 日志  
- [ ] 域名、ClientID 等从配置读取，默认值为候选（见 04）
