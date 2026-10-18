# HANDOFF → Architect（签到编排合同）

> 作者：签到执行子代理 · 日期：2026-10-18（Asia/Shanghai）  
> 范围：WorkBuddy「Buddy 加油站」+ TraeCode CN / TRAE · Tauri+Rust 同进程  
> 约束：未验证项保持配置化；**禁止文档/日志写 token**。

---
> **2026-10-18 续：** 已落地 Runtime 门控（默认 DryRun / `allow_live_http=false`） + `CredentialsRef.uri` ↔ `credential-vault`（`vault_bridge::resolve_for_checkin`）；详见 [06-runtime-gate-and-vault.md](./06-runtime-gate-and-vault.md)。
>
> **2026-10-18 续 2：** **可 import .info / Trae storage.json → vault**（`import_desktop` + CLI `import_desktop`）；本机路径只作导入源；真 HTTP 仍 stub。详见 [07-import-desktop.md](./07-import-desktop.md)。
> **2026-10-18 续 3：** `import_both_defaults` / `ImportBothReport` 一键双导入已交付；CLI `--platform both`。详见 [07-import-desktop.md](./07-import-desktop.md)。
> **2026-10-18 续 4：** Auth/Frontend 联调清单 [08-auth-integration-checklist.md](./08-auth-integration-checklist.md)（CredMan 2560 / `payload_too_large` 启发式；等 Auth `TooLarge`）。真 HTTP 仍关；不做本机打包。
> **2026-10-18 续 5：** 对齐 Auth blob>2560：`format_import_error` 按 `StoreError::code()` + 中文建议；Trae 无 `vault_dir` 早失败 `VaultDirRequired`；`payload_too_large` 仅残余。真 HTTP 仍关；不打包。
> **2026-10-18 续 6：** LiveHttp adapter 已落地（WB daily-checkin + Trae claim）；门控默认关；wiremock 测绿；Trae 可选 JWT 入仓（cipher-only → `trae_jwt_unavailable`）。未打包、未对生产域 live 验收。


## 0. 本机验证补丁（Architect 探测 · 2026-10-18）

已升格为 **本机 Windows 已验证默认**（仍可配置覆盖）：

| 项 | 已验证值 |
|----|----------|
| WB 登录态相对路径 | `%LOCALAPPDATA%\CodeBuddyExtension\Data\Public\auth\workbuddy-desktop.info` |
| WB auth | JWT 明文存在于 auth 字段（**不写入仓库/文档示例**） |
| WB domain | `copilot.tencent.com` → **base_url 默认升格为 `https://copilot.tencent.com`**；备选 `https://www.codebuddy.cn` |
| Trae storage | `%APPDATA%\Trae CN\User\globalStorage\storage.json` |
| Trae 鉴权键提示 | `iCubeAuthInfo://icube.cloudide` |
| Trae deviceId 键 | `telemetry.devDeviceId` |
| TRAE SOLO / 国际 Trae | **本机不存在** → v1 不做 |

crate：`DesktopPaths` + `probe`（dry_run 实读路径，只回 mask 摘要）。

### Architect 代定（产品）

1. 首期：仅 WorkBuddy 桌面 + Trae CN；Windows 优先  
2. 凭据：优先读本机已验证路径并托管；允许后续导入  
3. 本期编排：仅 daily-checkin（不含旅行/补登）  
4. WB base 默认 copilot；备选 codebuddy 配置化  
5. Trae：配对本机 deviceId；ClientID 默认候选 `ono9krqynydwx5` 仍配置化  
6. 多号同出口：concurrency ≤ 3 + jitter（Orchestrator 默认已具备）  
7. Q10：v1 记 `credits_delta`；`fetch_points` 尽力；entitlement 非必须  

---

## 1. 合同索引

| 文件 | 内容 |
|------|------|
| [01-platform-adapter.md](./01-platform-adapter.md) | `PlatformAdapter` trait；`CheckinCtx`；`CheckinOutcome`；幂等；`AdapterError` |
| [02-orchestrator.md](./02-orchestrator.md) | `run_all`；concurrency=3 + jitter；事件；与 Switcher/Monitor 边界 |
| [03-schema.md](./03-schema.md) | `checkin_run` / `checkin_item`；AlreadyDone 记账差异 |
| [04-platforms.md](./04-platforms.md) | WB / Trae 路径·材料·风险（已验证默认 + 候选） |
| [05-integration.md](./05-integration.md) | 脚本 vs 桌面；`checkin_run_all`；凭据边界 |
| [06-runtime-gate-and-vault.md](./06-runtime-gate-and-vault.md) | RuntimeMode 门控；CredentialsRef↔vault |
| [07-import-desktop.md](./07-import-desktop.md) | 本机 .info / storage.json → vault 导入 |
| [rust-sketch/adapter.rs](./rust-sketch/adapter.rs) | trait + struct + 码表映射草稿 |

Python 对照：`/workspace/checkin-suite/checkin/`（非桌面依赖）。  
Rust crate：`/workspace/checkin-suite/crates/checkin-core/`。

---

## 2. 写死要点（可直接开工）

| 项 | 值 |
|----|-----|
| **幂等键** | `(account_id, local_day, platform)`；时区默认 `Asia/Shanghai` |
| **已签业务码** | WorkBuddy `10001` → AlreadyDone；Trae `9095` → AlreadyDone |
| **事件** | `checkin:progress` / `checkin:done` |
| **表** | `checkin_run` / `checkin_item` |
| **Tauri command** | `checkin_run_all` |
| **并发默认** | `concurrency=3`，可配 `jitter_ms`（多号同出口） |
| **记账** | 仅 `Success.credits_delta` 计入日汇总；`AlreadyDone` 不累加 |
| **判定** | 平台业务码优先；禁止仅靠 `today_checked_in` 预判 |
| **编排范围** | **仅 daily-checkin** |

---

## 3. 默认配置现状

| 项 | 默认 | 状态 |
|----|------|------|
| WB base | `https://copilot.tencent.com` | **本机已验证默认** |
| WB alt | `https://www.codebuddy.cn` | 候选备选 |
| Trae ug / oauth | `https://api.trae.cn` / `https://api.trae.com.cn` | 候选·未拍板 |
| Trae ClientID | `ono9krqynydwx5` | 默认候选，仍配置化 |
| WB / Trae 桌面相对路径 | 见 §0 | **本机已验证默认** |

一律仍可配置覆盖。

---

## 4. 待用户确认 Q1–Q10

| Q | 状态 |
|---|------|
| **Q1** WorkBuddy 桌面 `.info` 路径？ | **部分已由 Architect 探测确认**（Windows 相对路径）；macOS/Linux 仍待 |
| **Q2** CN vs Global？ | 仍待 |
| **Q3** 凭据：桌面态 vs accounts.json？ | **代定：优先本机路径托管，允许导入**；细部仍可问 |
| **Q4** 是否兼容旧 vscdb？ | 仍待（v1 可不做） |
| **Q5** 是否含旅行/补登？ | **代定：不含** |
| **Q6** Trae 凭据来源 / 是否加密？ | **部分已确认路径与键名**；加密与否仍待 |
| **Q7** 真实 deviceId？ | **代定：配对本机 `telemetry.devDeviceId`** |
| **Q8** ClientID？ | **代定默认候选 `ono9krqynydwx5`，仍配置化** |
| **Q9** 多号同 IP？ | **代定：concurrency≤3 + jitter** |
| **Q10** entitlement？ | **代定：v1 记 credits_delta；fetch_points 尽力；entitlement 非必须** |

请 Architect 转达仍待项（勿让执行代理直接问用户）。

---

## 5. 建议下一步（Architect 排期）

1. ~~本机路径 → vault 导入~~ **已交付**（含 `import_both_defaults`）。  
2. Auth Win CredMan + Frontend IPC（见 08 清单）；用户明确验收再开 `allow_live_http`。  
3. ~~实装 WB / Trae HTTP~~ **已交付（门控默认关；装包前勿开 allow_live_http）**。  
4. Monitor 订阅 `checkin:done`。

---

## 6. 明确不做

- 不在合同中写死未确认的**绝对**用户路径。  
- 不把 token 写入事件、表外日志或 git。  
- Orchestrator 不换号、不做长驻监控。  
- v1 不做旅行/补登、不做国际 Trae / TRAE SOLO。
