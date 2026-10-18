# Buddy 加油站 (BuddyGasStation)

WorkBuddy + Trae CN 本地签到与换号桌面端（Tauri 2 + React）。

## 布局

- `apps/desktop` — Tauri 桌面壳与前端
- `checkin-suite` — Rust 签到/凭据核心 crates
- `.github/workflows/desktop-windows.yml` — Windows NSIS/MSI 出包

## 构建

由 GitHub Actions 出 Windows 安装包；勿在用户本机打包。

productName=`BuddyGasStation`，identifier=`com.benchforge.buddy-gasstation`。
