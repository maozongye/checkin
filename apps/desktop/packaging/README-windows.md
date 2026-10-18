# Buddy 加油站 — Windows 安装包（CI 出包）

> Architect 拍板：identifier `com.benchforge.buddy-gasstation` · `installMode=currentUser` · targets `nsis`（主）+ `msi`（附）· **无签名** · 占位图标。  
> **产品约束（用户确认）**：禁止在用户本机打包；功能写完后由 **CI/构建机** 出 NSIS，用户只下载安装。

## 用户怎么拿到安装包

1. 等 main（或指定 tag）上 `desktop-windows` workflow 跑绿。
2. 打开该次 GitHub Actions run → **Artifacts** → 下载 `BuddyGasStation-windows-<sha>`。
3. 解压后运行 `BuddyGasStation_*_x64-setup.exe`（NSIS 主交付；同包内可能有 `.msi`）。
4. **未签名**：SmartScreen 可能提示「未知发布者」→「更多信息」→「仍要运行」。

不要让用户在自己电脑上执行 `tauri build` / 装 Rust 工具链作为常规路径。

## 维护者：触发 CI

- **自动**：push / PR 变更 `apps/desktop/**` 或本 workflow。
- **手动**：Actions → `desktop-windows` → `Run workflow`（可选只打 `nsis`）。

Artifact 保留 **14 天**。需要更长分发再挂 GitHub Release（另开）。

## 仓库内配置位置

| 路径 | 说明 |
|------|------|
| `apps/desktop/src-tauri/tauri.conf.json` | 已合并的正式 conf |
| `apps/desktop/packaging/tauri.conf.json` | Ops 草稿备份（CI **不再**覆盖正式 conf） |
| `.github/workflows/desktop-windows.yml` | Windows 构建机出包 |

## CI 就绪状态（Ops）

| 项 | 状态 |
|----|------|
| `src-tauri` + conf + `icon.ico` | 已有 |
| workflow 上传可下载 artifact | 已有（单 artifact 含 NSIS，MSI 有则附带） |
| 无签名 / SmartScreen 说明 | 本文 |
| `package-lock.json` | **缺** — CI 走 `npm install`；建议 Frontend 提交 lock 后再开 npm cache |
| checkin / CredMan / 真 IPC | **未合入** — 当前 stub（`greet`）应仍能编出安装壳；功能验收打包等 IPC 合入后再打正式包 |
| 本 Linux box 实打 .exe | **不做**（无 Windows runner）；只维护 CI |

## 签名（后续）

生产再上 OV/EV；配置 `certificateThumbprint` + CI 证书密钥。私钥不进仓库。
