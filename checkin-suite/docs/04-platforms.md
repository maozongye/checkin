# 04 · 平台适配说明（WorkBuddy / Trae CN）

> **v1 产品范围（Architect 代定）**  
> - 仅 **WorkBuddy 桌面** + **Trae CN**；**Windows 优先**（无 TRAE SOLO / 国际 Trae）  
> - 凭据：**优先读本机已验证路径并托管**；允许后续导入  
> - 编排：**仅 daily-checkin**（不含旅行/补登）  
> - 积分：v1 **记 `credits_delta`**；`fetch_points` 尽力；entitlement **非必须**  
> - 多号同出口：`concurrency ≤ 3` + jitter  
>
> **本机 Windows 已验证默认**（Architect 探测，仍可配置覆盖）与 **候选·未拍板** 分栏标注。  
> **禁止**在文档、日志、事件中写入 token / JWT 明文。

---

## A. WorkBuddy（Buddy 加油站）

### A.1 官方路径

| 项 | 默认 | 状态 |
|----|------|------|
| CN Base | `https://copilot.tencent.com` | **本机已验证默认**（登录态 domain 命中；仍可配置） |
| 备选 Base | `https://www.codebuddy.cn` | 候选备选（仍可配置） |
| Global | `https://www.workbuddy.ai` | **不参与签到**，Adapter 返回 `Skipped` |
| 查状态 | `POST {base}/v2/billing/meter/checkin-activity-status` body `{}` | 候选路径，待实测 |
| 签到（本期唯一） | `POST {base}/v2/billing/meter/daily-checkin` body `{}` | 候选路径；**不含旅行/补登** |

**Headers（语义写死，值来自凭据）：**

```
Authorization: Bearer <accessToken>
X-User-Id: <uid>
Content-Type: application/json
```

**业务码（对照 Python，待实测复核）：**

| 码 | 含义 | Outcome |
|----|------|---------|
| `0` | 成功 | `Success`（记 `credits_delta`） |
| `10001` | 今日已签 | `AlreadyDone` |
| `12153` | 会话失效 | `Disabled` |

幂等主路径：**直接** `daily-checkin`，以 `0` / `10001` 判定。  
`checkin-activity-status` 的 `today_checked_in` **不可靠**，仅可展示，禁止作跳过依据。

配置示例：

```json
{
  "workbuddy": {
    "base_url": "https://copilot.tencent.com",
    "alt_base_url": "https://www.codebuddy.cn",
    "_comment": "base=本机已验证默认；alt=候选备选；均可覆盖"
  }
}
```

### A.2 登录态材料

| 材料 | 用途 | 来源 |
|------|------|------|
| `uid` / `X-User-Id` | 头 | 本机 `.info` 或导入 |
| `accessToken` | Bearer | 同上（JWT 明文存在 auth 字段；**勿写入日志**） |
| `refreshToken` | 过期续期 | 同上；续期接口路径 **待确认** |
| `domain` | 选 base / 识别 Global | 本机探测见 `copilot.tencent.com` |

#### 桌面落盘路径（Windows）

| 项 | 值 | 状态 |
|----|-----|------|
| 相对 `%LOCALAPPDATA%` | `CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info` | **本机已验证默认**（仍可配置） |
| 结构提示 | `account.uid`；`auth` 为 JWT 明文或含 `accessToken`/`domain` | 已验证存在；**文档不写 token** |
| 无 TRAE SOLO / 国际 Trae | — | 本机已确认不在 v1 范围 |

凭据策略：优先读上述路径并托管；允许后续 JSON / Switcher 导入。

dry_run 路径探测（`probe`）：只返回 `exists` / `uid_present` / `domain` / `has_access_token` 等摘要，**永不返回 JWT 明文**。

### A.3 已知风险

1. **域名改版**：copilot.tencent ↔ codebuddy → base 必须可配置。  
2. **`today_checked_in` 不可靠** → 禁止预判漏签。  
3. **Token 过期**：需开官方客户端重新登录，或走 refresh（接口未文档化，**待确认**）。  
4. **Global 账号**误入签到队列 → 必须 `Skipped`。  
5. **协议未文档化**：字段名变更即 Protocol 错误；保持 raw 日志（脱敏）便于改版。

---

## B. Trae CN / TRAE

### B.1 官方路径（候选默认）

| 项 | 候选默认 | 状态 |
|----|----------|------|
| UgHost | `https://api.trae.cn` | 已知候选 / 未拍板 |
| OAuthHost | `https://api.trae.com.cn` | 已知候选 / 未拍板 |
| 状态 | `POST {ug}/trae/api/v2/ug/checkin_credits/status` | 候选，待实测 |
| 领取（本期唯一） | `POST {ug}/trae/api/v2/ug/checkin_credits/claim` | 候选；daily only |
| Body | `{"req_source": 1}` | 对照实现常用值，**待确认** |
| 网络 | **强制直连**（绕过系统代理） | 强烈建议默认 true |

**Auth：**

```
Authorization: Cloud-IDE-JWT <accessToken>
```

注意：`Cloud-IDE-JWT` 是 Authorization **值的前缀**；若存储已含此前缀则勿重复拼接。

**必需 / 建议头：**

| Header | 说明 |
|--------|------|
| `x-device-id` | **必须**为本机 `telemetry.devDeviceId`（已验证键）；假值易 `9004` |
| `x-device-type` | 建议 `windows`（或与客户端一致，待确认） |

只发小写 `x-device-id`（对照实现约定）。

**业务码（对照 Python，待实测复核）：**

| 码 | 含义 | Outcome / Error |
|----|------|-----------------|
| `0` | 成功 | `Success`（记 `credits_delta`） |
| `9095` | 今日已签 | `AlreadyDone` |
| `9074` | 限流 | `Failed { retryable: true }` + 退避；映射 `RateLimited` |
| `9004` | 设备无效 | `Failed { retryable: false }` / `Config` |
| `1001`/`1002`/`401`/`403` | 鉴权失败 | refresh 或 `AuthExpired`/`AuthInvalid` |

### B.2 ClientID（续期 / OAuth）

| 候选 | 状态 |
|------|------|
| `ono9krqynydwx5` | **默认候选**（Architect 代定，仍配置化） |
| `en1oxy7wnw8j9n` | 旧版候选 / 未拍板 |

### B.3 登录态材料与桌面路径

| 材料 | 必需 | 说明 |
|------|------|------|
| `accessToken` | 是 | Cloud-IDE-JWT |
| `refreshToken` | 强烈建议 | ExchangeToken 续期（路径/字段 **待确认**） |
| `deviceId` | **claim 必需** | 配对 `telemetry.devDeviceId` |
| `clientId` | 续期需要 | 默认候选 `ono9krqynydwx5` |

#### 桌面落盘路径（Windows）

| 项 | 值 | 状态 |
|----|-----|------|
| 相对 `%APPDATA%`（Roaming） | `Trae CN/User/globalStorage/storage.json` | **本机已验证默认** |
| 鉴权键提示 | `iCubeAuthInfo://icube.cloudide` | **本机已验证**（键名，非密钥） |
| 设备号键 | `telemetry.devDeviceId` | **本机已验证** |
| 国际 Trae / TRAE SOLO | — | **本机不存在**；v1 不支持 |

凭据：优先读本机路径并托管；允许后续导入。storage 可能加密 — Switcher 负责解密/注入；签到模块只消费 `credentials_ref`。

### B.4 已知风险

1. **`9004`**：伪造 / 过期 deviceId。  
2. **`9074`**：同 IP 多号 → **concurrency ≤ 3 + jitter**（Orchestrator 默认已具备）。  
3. **ClientID 变更**：续期失败；双候选可回退但须实测。  
4. **奖励口径**：v1 只保证 `credits_delta`；余额接口尽力。  
5. **协议未文档化**：靠脱敏 raw 排障。

---

## C. 两边共用实现注意

1. HTTP 非 2xx 时仍尝试解析 JSON 业务码（尤其 400）。  
2. 日志 mask token（仅前后 4 字符或不写任何 token 字符）。  
3. `fetch_points` / `health` 失败不必然阻断同账号 `checkin`。  
4. 所有可配置默认集中在 `PlatformConfig` + `DesktopPaths`。  
5. dry_run：可实读本机路径做结构探测；返回 `PathProbeSummary`（无 JWT）。
