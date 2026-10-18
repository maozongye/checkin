# checkin-core

WorkBuddy「Buddy 加油站」+ Trae CN 每日签到的 **Rust 桌面 crate stub**（dry_run 优先）。

接口层按 `docs/01–03`、`05`、`rust-sketch` 定稿；`04-platforms` 的域名 / ClientID / 路径全部从 `PlatformConfig` 读取，默认值旁标注 **候选·未拍板**。

## 构建 / 检查

```bash
cd /workspace/checkin-suite/crates/checkin-core
cargo check
cargo test
```

## dry_run 用法

`dry_run=true` 时 Adapter **不发 HTTP**，直接返回 `Skipped { reason: "dry_run" }`；Orchestrator 仍会写 `checkin_run`（`note=dry_run`）与 `checkin_item`。

```rust
use checkin_core::*;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Arc::new(Mutex::new(CheckinDb::open_in_memory()?));
    let orch = CheckinOrchestrator::with_default_adapters(OrchestratorConfig {
        jitter_ms: 0, // 测试可关抖动
        ..Default::default()
    });

    let accounts = vec![
        AccountSpec {
            account_id: "wb-001".into(),
            platform: "workbuddy".into(),
            credentials_ref: CredentialsRef {
                store: "accounts_json".into(),
                key: "wb-001".into(),
            },
            enabled: true,
        },
        AccountSpec {
            account_id: "trae-001".into(),
            platform: "trae".into(),
            credentials_ref: CredentialsRef {
                store: "accounts_json".into(),
                key: "trae-001".into(),
            },
            enabled: true,
        },
    ];

    let events = Arc::new(StdMutex::new(Vec::new()));
    let ev2 = events.clone();
    let summary = orch
        .run_all(db, &accounts, Trigger::Manual, true, move |e| {
            ev2.lock().unwrap().push(e.name().to_string());
        })
        .await?;

    println!("status={} skipped={}", summary.status, summary.counts.skipped);
    Ok(())
}
```

## 凭据

只通过 `CredentialsRef { store, key }` 注入；**禁止**把 token 写入事件、日志或本 crate 常量。本机绝对路径不写死，由 Switcher / 桌面端 resolve。

## 事件名（写死）

- `checkin:progress`
- `checkin:done`

桌面端把 `emit` 回调接到 `app.emit(...)` 即可。

## 真请求

`workbuddy.rs` / `trae.rs` 内留有 TODO；接入时用 `PlatformConfig` 覆盖 base_url / client_id，再加 HTTP 客户端（本 stub **不**依赖 reqwest）。
