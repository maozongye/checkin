# Buddy 加油站 — 桌面端

正式 App 名：**Buddy 加油站**  
打包名（ASCII）：`BuddyGasStation` · identifier：`com.benchforge.buddy-gasstation`

技术栈：Tauri 2 + React + TypeScript + Vite + Tailwind（shadcn 风格）。

建议 Node.js **18+**；Windows 正式打包需 Rust + WebView2。

## 纯前端预览（无 Rust）

```bash
cd apps/desktop
npm install
npm run dev    # http://localhost:1420 ；走 lib/api mock
```

## Tauri 桌面

`src-tauri` 已注册全部 IPC command（数据仍 mock，进度事件可 emit）。**当前不打包**；先写完代码。

```bash
cd apps/desktop
npm install
npm run tauri dev     # 开发窗，标题「Buddy 加油站」；走真实 command 通道
# npm run tauri build  # 暂勿执行；等代码写完 + Ops 打包
```

已注册 command：`account_list` / `checkin_run_all` / `scheduler_get|set` / `switch_*`（`switch.rs` 由 Switcher 维护） / `monitor_*` / `credential_store`（+ `greet`）。  
事件：`checkin:progress` / `checkin:done` / `switch://progress` / `monitor:alert`。数据仍 mock；**不打包**。

脚本：`package.json` → `"tauri": "tauri"`（`@tauri-apps/cli`）。  
Vite `server.port` = **1420**，与 `src-tauri/tauri.conf.json` → `build.devUrl` 对齐。

## 打包说明（Ops）

见 `packaging/README-windows.md`。合并步骤已完成：

1. `cp packaging/tauri.conf.json src-tauri/tauri.conf.json`
2. `npx @tauri-apps/cli icon packaging/icons/icon-source.png` → `src-tauri/icons/`

**骨架已立，未在本机验证 Windows NSIS 实装。**
