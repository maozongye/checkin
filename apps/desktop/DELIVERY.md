# Buddy 加油站 桌面端 — 第一刀竖切交付说明

> 路径：`/workspace/apps/desktop/`  
> 技术栈：Tauri 2 + React + TypeScript + Vite + Tailwind + shadcn 风格（手写最小 UI）  
> 风格：近黑后台 · 白/灰字 · 成功绿 / 失败红  
> 日期：2026-10-18（Asia/Shanghai）

---

## 1. 组件树

```
App (BrowserRouter + useRoutes)
└── AppShell
    ├── Sidebar                    # 7 路由导航，可折叠
    └── <Outlet>
        ├── OverviewPage           # 概览 + 一键签到 CTA
        │   ├── PageHeader
        │   ├── StatCard ×3
        │   ├── CheckinButton
        │   └── CheckinProgress    # ← useCheckinProgress
        ├── AccountsPage           # 账号表（脱敏）
        ├── CheckinPage            # 签到主战场
        │   ├── CheckinButton
        │   └── CheckinProgress
        ├── SchedulerPage          # 定时开关（mock）
        ├── SwitcherPage           # 换号：预检 → Confirm → progress → 结果
        │   ├── ConfirmSwitchDialog
        │   └── SwitchProgress
        ├── MonitorPage            # 监控看板（Architect 合同）
        │   ├── SummaryCard ×3 + updatedAt
        │   ├── Alert toasts (listen monitor:alert)
        │   ├── Tab: 账号列表 | 趋势(占位)
        │   ├── 账号表格行 → 点击
        │   └── DetailDrawer       # 底部弱链接：去签到 / 去换号
        └── SettingsPage           # v1 默认文案
```

**共享层**

| 模块 | 职责 |
|------|------|
| `lib/api.ts` | invoke 封装；无 Tauri 时 mock；listen 事件 |
| `lib/types.ts` | IPC 可见 DTO（无凭据） |
| `lib/utils.ts` | `cn()` |
| `hooks/useCheckinProgress.ts` | 订阅 `checkin:progress` / `checkin:done`，触发 `checkin_run_all` |
| `theme/tokens.css` | 黑白语义色 tokens |
| `components/ui/*` | Button / Progress / Badge（手写 shadcn 风格） |

---

## 2. 路由与侧栏

| 路径 | 页面 | 侧栏文案 |
|------|------|----------|
| `/` | OverviewPage | 概览 |
| `/accounts` | AccountsPage | 账号 |
| `/checkin` | CheckinPage | 签到 |
| `/scheduler` | SchedulerPage | 定时 |
| `/switcher` | SwitcherPage | 换号 |
| `/monitor` | MonitorPage | 监控 |
| `/settings` | SettingsPage | 设置 |

侧栏：Lucide 图标 + 文案；`NavLink` 高亮；折叠后仅图标。

---

## 3. 各页线框说明

### 概览 `/`
- 顶栏：标题 + **一键签到**主按钮
- 三张统计卡：启用账号 / 会话健康 / 积分合计（读 `account_list` + `monitor_get_dashboard`）
- 下方：签到进度区（与签到页共用组件）

### 账号 `/accounts`
- 表头：名称 / 平台 / 状态 / 积分 / 连签 / 标签
- 「导入账号」按钮 disabled 占位
- **不展示**任何 token / cookie

### 签到 `/checkin`
- 主 CTA：一键签到 + 重置
- 进度条 + 摘要徽章 + 结果列表
- 历史 run 虚线占位

### 定时 `/scheduler`
- 展示 enabled / 时间 / 时区 / 平台过滤
- 「开启/关闭定时」调 `scheduler_set`

### 换号 `/switcher`（Switcher 合同）
- 选账号 → `switch_precheck` → ConfirmUI（必显「只切登录态，不迁 Chat/工作区」）
- `clientRunning` 禁用确认；listen `switch://progress`；`rollback_failed` 可重试回滚
- 支持 `?accountId=`（Monitor 抽屉弱链接）；详见 `SWITCHER.md`

### 监控 `/monitor`（Architect 定稿映射）
见 §5。

### 设置 `/settings`
- KV 行：主题（仅深色）/ 字体（系统）/ 窗口（原生标题栏）/ 签到取消（v1 不支持）/ 并发 / 数据目录 / 导出日志
- Danger Zone 占位

---

## 4. 关键组件 Props

### `CheckinButton`
```ts
{
  loading: boolean;       // 禁用重复点击 + spinner
  onClick: () => void;
  label?: string;         // 默认「一键签到」
  className?: string;
  size?: "default" | "sm" | "lg";
  disabled?: boolean;
}
```

### `CheckinProgress`
```ts
{
  phase: "idle" | "running" | "done" | "error";
  done: number;
  total: number;
  currentAccount?: string;
  status?: CheckinItemStatus;
  summary?: CheckinDoneEvent | null;
  error?: string | null;
  className?: string;
}
```

### `useCheckinProgress()` →
```ts
{
  phase, jobId, done, total, currentAccount, status, summary, error,
  runAll(opts?), reset(), isRunning
}
```

### `ConfirmSwitchDialog`
```ts
{
  open: boolean;
  precheck: SwitchPrecheckResult | null;
  confirming: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}
```

### `SwitchProgress`
```ts
{
  phase: "idle" | "running" | "done" | "error";
  progress: SwitchProgressEvent | null;
  resultKey?: SwitchMessageKey | null;
  error?: string | null;
}
```

### `Sidebar`
```ts
{ collapsed: boolean; onToggle: () => void }
```

### `PageHeader`
```ts
{ title: string; description?: string; actions?: ReactNode }
```

### `Progress`
```ts
{ value: number; max?: number; tone?: "default" | "success" | "danger" }
```

---

## 5. Monitor 看板映射（合同 → UI）

| 合同字段 | UI 位置 |
|----------|---------|
| `summary.healthy / warn / error` | 顶栏三张 SummaryCard |
| `updatedAt` | 顶栏「更新时间」（Asia/Shanghai 格式化） |
| `accounts[].displayName` | 列表「账号」列 |
| `accounts[].provider` | 列表「平台」列（workbuddy → WorkBuddy） |
| `accounts[].pointsBalance` | 列表「积分」 |
| `accounts[].streakDays` | 列表「连签」 |
| `accounts[].session.status` | 色点 + 文案（ok 绿 / warn 黄 / error\|need_relogin 红） |
| `accounts[].lastCheckin.status === failed\|need_relogin` | 「fail」角标 Badge |
| 行点击 | 右侧 DetailDrawer |
| `pointsExpireAt` | 抽屉 · 积分过期 |
| `session.expiresAt` / `session.lastCheckedAt` | 抽屉 · 会话 |
| `lastCheckin.*` | 抽屉 · 最近签到详情 |
| 进页 | `monitor_get_dashboard` |
| 刷新按钮 | `monitor_refresh` |
| `listen("monitor:alert")` | 页顶可关闭 toast 条 |
| 趋势 | Tab「趋势」虚线占位（v1 不画） |
| 抽屉底部弱链接 | 「去签到」→ `/checkin`；「去换号」→ `/switcher?accountId=` |

**API**

- `monitorGetDashboard()` → `monitor_get_dashboard`
- `monitorRefresh()` → `monitor_refresh`
- `EVENTS.monitorAlert` = `"monitor:alert"`

**DTO**（`lib/types.ts`）：`MonitorDashboard` / `MonitorSummary` / `MonitorAccountRow` / `MonitorSession` / `MonitorLastCheckin` / `MonitorAlert`

---

## 6. lib/api 命令与事件一览

| Command | 函数 | Mock |
|---------|------|------|
| `account_list` | `accountList` | 4 条脱敏账号 |
| `checkin_run_all` | `checkinRunAll` | 返回 jobId，异步 emit progress/done |
| `scheduler_get` / `scheduler_set` | `schedulerGet` / `schedulerSet` | 内存态 |
| `switch_precheck` | `switchPrecheck` | 预检；`acc-wb-2` / `?forceRunning=1` → clientRunning |
| `switch_confirm_exec` | `switchConfirmExec` | 返回 jobId，异步 emit `switch://progress` |
| `switch_rollback_retry` | `switchRollbackRetry` | mock 成功 → rolled_back |
| `switch_list` | `switchList` | 可换号账号（同 mock 列表） |
| `monitor_get_dashboard` | `monitorGetDashboard` | 完整 Dashboard DTO |
| `monitor_refresh` | `monitorRefresh` | 刷新 + emit 一条 info alert |
| `credential_store` | `storeCredential` | Auth IPC：`{ args }` → `CredentialStoreOk`；丢弃 secret_json |
| `credential_delete` | `deleteCredential` | Auth IPC：需 backup_password；不回传 secret |

> **合同更名**：旧 `switcher_switch` / `switcherSwitch` 已移除，请用上表 `switch_*`。

| Event | 用途 |
|-------|------|
| `checkin:progress` | 进度条推进 |
| `checkin:done` | 摘要 + 结果列表 |
| `monitor:alert` | 监控页 toast |
| `switch://progress` | 换号阶段进度（含 `://`） |

无 `__TAURI_INTERNALS__` 时全部走 mock，可用 `npm run dev` 纯 Vite 预览（本刀未执行 install）。

---

## 7. 主题 tokens

| Token | 示例值 | 用途 |
|-------|--------|------|
| `--bg` | `#0a0a0a` | 页面底 |
| `--bg-elevated` | `#111111` | 卡片/侧栏 |
| `--border` | `#262626` | 分割线 |
| `--fg` / `--fg-muted` / `--fg-subtle` | `#fafafa` / `#a1a1aa` / `#71717a` | 文字层级 |
| `--accent` / `--accent-fg` | `#fafafa` / `#0a0a0a` | 主按钮（白底黑字） |
| `--success` / `--danger` / `--warning` | 绿/红/黄 | 语义 |

---

## 8. 假设与阻塞 / 待 Architect 转用户的问题

### Architect / 用户已确认
1. **App 名**：Buddy 加油站（文字标；折叠侧栏用「B」）。
2. **主题**：仅深色（不做浅色 / 系统跟随）。
3. **字体**：系统字体栈。
4. **窗口**：系统原生标题栏（不自定义铬条）。
5. **签到取消**：不做 `checkin_cancel`；进行中任务不可取消。
6. **Monitor 抽屉弱链接**：已加「去签到」「去换号」（弱链接，非大按钮）。

### 本刀其它假设
1. **品牌色**：无独立品牌色，主 CTA 用白底黑字；语义色仅绿/红/黄。
2. **Monitor provider 字段名**：前端 DTO 用 `provider`（与合同一致）；账号列表仍用 `platform`。
3. **时间展示**：一律按 `Asia/Shanghai` `toLocaleString("zh-CN")`。
4. **换号 ConfirmUI**：已按合同实现（含 client_running 禁用）。

### 仍等用户确认
1. 独立 **Logo**（暂无则继续文字标「Buddy 加油站」/「B」）。

### 明确不做（本刀）
- **不打包**（`tauri build` / NSIS / MSI）
- 真实签到逻辑、凭据明文处理（前端仅 store/delete ref；vault 由 Auth 注入）
- 消耗趋势图 / `checkin_cancel`

---

## 9. 文件清单

```
apps/desktop/
  package.json
  tsconfig.json
  tsconfig.node.json
  vite.config.ts
  tailwind.config.ts
  postcss.config.js
  index.html
  components.json
  DELIVERY.md
  README.md
  src/
    main.tsx
    App.tsx
    index.css
    vite-env.d.ts
    routes.tsx
    lib/api.ts
    lib/types.ts
    lib/switchMessages.ts
    lib/credentialErrors.ts
    lib/utils.ts
    SWITCHER.md
    theme/tokens.css
    hooks/useCheckinProgress.ts
    components/layout/AppShell.tsx
    components/layout/Sidebar.tsx
    components/layout/PageHeader.tsx
    components/checkin/CheckinButton.tsx
    components/checkin/CheckinProgress.tsx
    components/switcher/ConfirmSwitchDialog.tsx
    components/switcher/SwitchProgress.tsx
    components/ui/button.tsx
    components/ui/progress.tsx
    components/ui/badge.tsx
    pages/OverviewPage.tsx
    pages/AccountsPage.tsx
    pages/CheckinPage.tsx
    pages/SchedulerPage.tsx
    pages/SwitcherPage.tsx
    pages/MonitorPage.tsx
    pages/SettingsPage.tsx
```

下一步建议：`npm install && npm run dev` 纯前端预览；Rust 侧按同名 command/event 接线后去掉 mock 分支即可。

## Tauri 2 src-tauri 骨架

> 2026-10-18 · Frontend scaffold + Ops conf 合并

- 路径：`/workspace/apps/desktop/src-tauri`
- productName=`BuddyGasStation` · identifier=`com.benchforge.buddy-gasstation` · 窗口标题=`Buddy 加油站`
- `tauri.conf.json`：已 `cp packaging/tauri.conf.json`（Ops 为准；targets nsis+msi；无签名）
- icons：`npx @tauri-apps/cli icon packaging/icons/icon-source.png` **成功**
- Rust：全部 command 已在 `generate_handler!` 注册（返回仍 mock；`checkin`/`switch`/`monitor` 会 emit）
- `credential_store` / `credential_delete`：对齐 Auth IPC 草案；mock 丢弃 secret_json；假 `cred://v1/account/mock-...`
- Vite 端口 1420 与 `devUrl` 已对齐
- **未在本机验证 Windows NSIS / MSI 实装**


## 10. Command 通道（2026-10-18 · 第二刀）

> 用户要求：先别打包，把代码写完。

- 路径：`/workspace/apps/desktop/src-tauri/src/commands/mod.rs` + `lib.rs`
- 数据：`commands/mock_data.rs`（与前端 JS mock 同 id）
- 状态：`AppState` = scheduler + credentials **meta**（无 secret）；真 vault 在 `VaultState`（Auth）
- **未执行** `tauri build` / 打包

## 11. Credential IPC 对齐（2026-10-18 · Frontend 收尾）

> 对齐 `docs/credential-tauri-ipc-draft.md` / `credential-store-error-codes-for-checkin.md`

- 前端类型：`CredentialStoreArgs` / `Ok` / `DeleteArgs` / `DeleteOk` / `CredentialIpcErr`（**snake_case wire**，与草案 + Auth Rust 一致）
- invoke：`storeCredential` → `credential_store({ args })`；`deleteCredential` → `credential_delete({ args })`
- 错误文案：`src/lib/credentialErrors.ts`（`formatCredentialError` / `parseCredentialInvokeError`）
- Rust：Auth 已落地真 vault（`commands/credential.rs` + `vault.rs` · `with_vault_dir`）；Frontend **未**再盖 mock stub
- `credential_meta_store`：Auth 保留的 meta-only list（无 UI 调用）；`AppState.credentials` 仅 meta
- 前端**不**构造 CredentialStore；成功后调用方丢弃 `secret_json`；Vite mock **不**把 secret 写入内存
- **未打包** / 未碰用户机；未改 `switch.rs`

