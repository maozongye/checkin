# WorkBuddy + Trae CN 每日签到套件

纯 **Python 3.10+ 标准库**，零第三方依赖。支持多账号批量、幂等跳过、Token 刷新、结果通知，以及手动 / 自动两种运行方式。

可独立脚本运行，也可被桌面端集成：

```python
from checkin.runner import run_all
summary = run_all(accounts_path="accounts.json", platform="all")
print(summary.human_line())
```

---

## v1 产品范围

- **平台**：仅 WorkBuddy 桌面 + Trae CN；**Windows 优先**（不做国际 Trae / TRAE SOLO）
- **编排**：仅 **daily-checkin**（不含旅行/补登）
- **凭据**：优先读本机已验证路径并托管；允许后续导入
- **积分**：记 `credits_delta`；`fetch_points` 尽力；entitlement 非必须
- **多号**：`concurrency ≤ 3` + jitter
- **安全**：日志/事件**绝不**含 token/JWT 明文；dry_run 可探测路径结构（mask 摘要）

## 目录结构

```
checkin-suite/
  README.md
  requirements.txt          # 标准库即可，无需安装
  accounts.example.json     # 多账号样例（无真实 token）
  config.example.json       # 通知 / 定时样例
  checkin/                  # 可 import 的核心包
  scripts/                  # CLI 与系统定时安装脚本
  tests/                    # unittest + mock，不访问外网
```

---

## 快速准备

```bash
cd /workspace/checkin-suite   # 或你的本地路径
cp accounts.example.json accounts.json
cp config.example.json config.json
# 编辑 accounts.json：填入 accessToken / refreshToken
# Trae 还必须填客户端真实注册过的 deviceId（否则 claim 返回 9004）
```

**切勿**把真实 `accounts.json` / 含 webhook 的 `config.json` 提交到仓库。

### 凭据来源说明

| 平台 | 推荐方式 |
|------|----------|
| WorkBuddy | `accounts.json`；或本机桌面端 `workbuddy-desktop.info`（见下方路径，需 `--from-desktop`） |
| Trae CN | 手工写入 `accessToken` / `refreshToken` / `deviceId`（桌面 Electron storage 可能加密，本脚本不强依赖解密） |

WorkBuddy 桌面端登录态路径：

- Windows: `%LOCALAPPDATA%\CodeBuddyExtension\Data\Public\auth\workbuddy-desktop.info`
- macOS: `~/Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info`

结构：`{ account: {uid, nickname}, auth: {accessToken, refreshToken, expiresAt, domain} }`

---

## 一、手动触发

适合：调试、补签、临时多账号跑一遍。

### 1. 一键签到

```bash
# 全部平台
python scripts/checkin_once.py --platform all

# 仅 WorkBuddy / 仅 Trae
python scripts/checkin_once.py --platform workbuddy
python scripts/checkin_once.py --platform trae --accounts ./accounts.json

# 演练（不发真实请求）
python scripts/checkin_once.py --dry-run

# JSON 输出 + 关闭通知
python scripts/checkin_once.py --json --no-notify

# 合并本机 WorkBuddy 桌面端登录态（可选）
python scripts/checkin_once.py --from-desktop -v
```

### 2. 作为模块调用

```python
from checkin.runner import run_all

summary = run_all(
    accounts_path="accounts.json",
    config_path="config.json",
    platform="all",   # all | workbuddy | trae
    dry_run=False,
    notify=True,
)
print(summary.human_line())
for r in summary.results:
    print(r.platform, r.nickname, r.status, r.message, r.credits_delta, r.streak_days)
```

### 3. 手动场景下的幂等行为

- **已签到**：WorkBuddy `code=10001`、Trae `code=9095` → 状态 `skipped`（视为成功，不报错）
- **可安全重跑**：同一天多次执行不会重复领积分
- **HTTP 400 + 业务码 10001**：仍按已签处理（以业务码为准，不以 HTTP 状态为准）

---

## 二、自动定时

时区一律按 **Asia/Shanghai（北京时间）**。下面三种任选其一。

### 方案 A：系统 cron（Linux / macOS）

```bash
chmod +x scripts/install_cron.sh
./scripts/install_cron.sh          # 默认每天 09:10
./scripts/install_cron.sh 08:30    # 自定义时刻
crontab -l                         # 确认
```

脚本会写入 `CRON_TZ=Asia/Shanghai`（若你的 cron 不支持该变量，请把系统时区设为上海，或自行换算本地时间）。

日志默认：`logs/cron.log`。

卸载示例：`crontab -e` 删掉带 `checkin_once.py` 的行。

### 方案 B：Windows 计划任务（schtasks）

在 PowerShell 中：

```powershell
cd C:\path\to\checkin-suite
.\scripts\install_schtasks.ps1
.\scripts\install_schtasks.ps1 -At "09:30"
Get-ScheduledTask -TaskName "CheckinSuite-WorkBuddy-Trae"
```

请将系统时区设为「中国」，或按本地时区调整 `-At`。

### 方案 C：内置 daemon（本地长期进程）

不依赖系统调度，进程常驻，到点自动跑：

```bash
python scripts/checkin_daemon.py --at 09:10 --platform all
# 启动时若已过今日触发点，先补跑一次：
python scripts/checkin_daemon.py --at 09:10 --once-if-missed
```

适合开发机挂起、或包装成 systemd / launchd / NSSM 服务。生产更推荐 A/B，进程崩溃不会漏调度。

### 通知（自动与手动通用）

编辑 `config.json` 的 `notify`：

- `local_log`：写入 `logs/checkin_YYYYMMDD_HHMMSS.json`
- `wecom_webhook`：企业微信群机器人 URL
- `pushplus_token` / `pushplus_topic`：PushPlus

也可用环境变量：`CHECKIN_WECOM_WEBHOOK`、`CHECKIN_PUSHPLUS_TOKEN`。

通知与日志**绝不**包含真实 token。

---

## 平台契约摘要

### WorkBuddy（Buddy 加油站）

| 项 | 值 |
|----|-----|
| CN Base（本机已验证默认） | `https://copilot.tencent.com` |
| 备选 | `https://www.codebuddy.cn` |
| Global | `https://www.workbuddy.ai` → **跳过不签** |
| 签到 | `POST /v2/billing/meter/daily-checkin` body `{}` |
| 状态（可选展示） | `POST /v2/billing/meter/checkin-activity-status` |
| Auth | `Authorization: Bearer <token>` + `X-User-Id` |
| 成功 / 已签 / 失效 | `0` / `10001` / `12153` |

**连签**：响应中的 `streak_days`、`today_credit` / `daily_credit` 会写入结果；`today_checked_in` 部分版本不可靠，故幂等主路径是**直接调 daily-checkin**，以 `0`/`10001` 判定。

### Trae CN

| 项 | 值 |
|----|-----|
| UgHost | `https://api.trae.cn` |
| OAuthHost | `https://api.trae.com.cn` |
| 领取 | `POST /trae/api/v2/ug/checkin_credits/claim` body `{"req_source":1}` |
| Auth | `Authorization: Cloud-IDE-JWT <token>`（前缀是值的一部分） |
| 设备头 | 小写 `x-device-id`（配对本机 `telemetry.devDeviceId`）+ `x-device-type` |
| ClientID | 默认候选 `ono9krqynydwx5`（仍配置化） |
| 成功 / 已签 / 限流 / 坏设备 | `0` / `9095` / `9074` / `9004` |

默认 **强制直连**（`ProxyHandler({})`），避免多账号共用代理 IP 触发 `9074`。  
`9074` 会指数退避重试；`refreshToken` 轮换成功后会回写 `accounts.json`。

---

## 测试

```bash
cd /workspace/checkin-suite
python -m unittest discover -s tests -v
```

全部使用 `unittest.mock`，**不访问外网**。覆盖：幂等码、HTTP 400+业务码、会话失效、Trae 设备号/限流、runner 多账号汇总、mask 不泄露 token。

---

## 设计要点

1. **幂等**：已签业务码 → `skipped` 且 `ok=True`；写操作可对幂等接口重试。
2. **错误码优先于 HTTP 状态**：尤其 WorkBuddy `400 + 10001`。
3. **Token 生命周期**：过期尝试 refresh；失败返回明确「客户端未登录/Token 过期」；WorkBuddy `12153` → `disabled`。
4. **安全**：`mask_secret` 不输出任何 token 字符；示例配置无真实密钥；默认不读本机真实登录态。
5. **可集成**：`from checkin.runner import run_all`；结果统一 `CheckinResult`。

---

## 许可证与免责

仅供个人学习与自动化自己的账号签到。请遵守各产品服务条款；勿分享、出售或滥用他人凭据。
