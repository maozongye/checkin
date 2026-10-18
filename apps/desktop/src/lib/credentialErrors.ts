/**
 * Credential IPC 错误码 → 中文用户文案。
 * 对齐 docs/credential-store-error-codes-for-checkin.md
 * 硬规则：文案禁止拼入 secret / token / cipher / file_key。
 */
import type { CredentialErrorCode, CredentialIpcErr } from "./types";

const USER_MESSAGES: Record<CredentialErrorCode, string> = {
  invalid_secret: "登录态格式不对，请重新从本机导入",
  replace_not_found: "原凭据不存在，请改为新建导入",
  vault_dir_required: "应用数据目录未就绪，请重启应用；若持续出现请反馈",
  payload_too_large: "备份内容过大，暂不支持裸写入（需 Switcher 指针化）",
  backend_unavailable: "系统凭据保险库不可用",
  backend_denied: "未获系统凭据权限，请在系统设置中允许",
  not_found: "凭据不存在或已删除",
  invalid_ref: "凭据引用无效",
  backup_required: "删除前请设置备份口令",
  corrupt_envelope: "凭据数据损坏，请重新导入",
  internal: "导入失败，请重试",
};

const KNOWN_CODES = Object.keys(USER_MESSAGES) as CredentialErrorCode[];

export function formatCredentialError(
  code: string | undefined | null,
  fallbackMessage?: string,
): string {
  if (code && code in USER_MESSAGES) {
    return USER_MESSAGES[code as CredentialErrorCode];
  }
  // 禁止把可能含 secret 的后端 message 原样展示；仅用安全兜底
  if (fallbackMessage && !looksLikeSecretLeak(fallbackMessage)) {
    return fallbackMessage;
  }
  return USER_MESSAGES.internal;
}

function looksLikeSecretLeak(msg: string): boolean {
  const lower = msg.toLowerCase();
  return (
    lower.includes("token") ||
    lower.includes("cipher") ||
    lower.includes("file_key") ||
    lower.includes("secret") ||
    lower.includes("password") ||
    /eyJ[A-Za-z0-9_-]{10,}/.test(msg) // JWT-ish
  );
}

/**
 * 从 Tauri invoke 抛错（string / Error / { code, message }）尽量抽出 code。
 */
export function parseCredentialInvokeError(e: unknown): CredentialIpcErr {
  if (e == null) {
    return { code: "internal", message: USER_MESSAGES.internal };
  }

  if (typeof e === "string") {
    const fromJson = tryParseIpcErrJson(e);
    if (fromJson) return fromJson;
    const code = extractCodeFromText(e);
    return {
      code: code ?? "internal",
      message: formatCredentialError(code, e),
    };
  }

  if (typeof e === "object") {
    const obj = e as Record<string, unknown>;
    // Tauri 有时包一层 { error: ... } 或直接 IpcErr
    const nested =
      typeof obj.error === "object" && obj.error != null
        ? (obj.error as Record<string, unknown>)
        : obj;

    const codeRaw =
      (typeof nested.code === "string" && nested.code) ||
      (typeof obj.code === "string" && obj.code) ||
      undefined;
    const messageRaw =
      (typeof nested.message === "string" && nested.message) ||
      (typeof obj.message === "string" && obj.message) ||
      undefined;

    if (codeRaw || messageRaw) {
      const code = codeRaw ?? extractCodeFromText(messageRaw ?? "") ?? "internal";
      return {
        code,
        message: formatCredentialError(code, messageRaw),
      };
    }

    if (e instanceof Error) {
      const code = extractCodeFromText(e.message);
      return {
        code: code ?? "internal",
        message: formatCredentialError(code, e.message),
      };
    }
  }

  return { code: "internal", message: USER_MESSAGES.internal };
}

function tryParseIpcErrJson(text: string): CredentialIpcErr | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    const parsed = JSON.parse(trimmed) as Record<string, unknown>;
    if (typeof parsed.code === "string") {
      return {
        code: parsed.code,
        message: formatCredentialError(
          parsed.code,
          typeof parsed.message === "string" ? parsed.message : undefined,
        ),
      };
    }
  } catch {
    /* ignore */
  }
  return null;
}

function extractCodeFromText(text: string): string | undefined {
  for (const code of KNOWN_CODES) {
    if (text.includes(code)) return code;
  }
  // 常见模式：code=invalid_secret 或 "code":"invalid_secret"
  const m =
    text.match(/["']?code["']?\s*[:=]\s*["']([a-z_]+)["']/i) ||
    text.match(/\b([a-z_]+)\b/);
  if (m?.[1] && KNOWN_CODES.includes(m[1] as CredentialErrorCode)) {
    return m[1];
  }
  return undefined;
}

export { USER_MESSAGES as CREDENTIAL_ERROR_MESSAGES };
