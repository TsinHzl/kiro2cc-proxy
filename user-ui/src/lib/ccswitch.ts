// Copyright (c) 2026 Harllan He. Licensed under MIT.

/**
 * cc-switch 一键导入：生成 `ccswitch://v1/import?...` 深链接
 *
 * 密钥由 cc-switch 桌面端存进自己的库，切换供应商时才写落 `~/.claude/settings.json`
 * 与 `~/.codex/config.toml`（Codex 侧注入 `experimental_bearer_token`），
 * 因此用户不需要再手写 `~/.codex/.env`。
 *
 * 注意：cc-switch 对 `app=codex` 会丢弃调用方传入的 TOML 正文，用固定模板重新生成，
 * 只认 name / endpoint / model / apiKey 四个值。所以这里只拼扁平 URL 参数，不用 `config=`。
 */

export type CcSwitchApp = 'claude' | 'codex'

/** 写进 Codex config.toml 的模型名；`gpt-5.6-` 前缀在 OpenAI 兼容层原样透传 */
const CODEX_MODEL = 'gpt-5.6-terra'

/** 余额自动查询间隔（分钟），cc-switch 上限 1440 */
const USAGE_AUTO_INTERVAL = '30'

/**
 * cc-switch 的余额查询脚本，请求 `GET /api/user/usage`。
 * `{{baseUrl}}` / `{{apiKey}}` 由 cc-switch 在执行前替换。
 */
const USAGE_SCRIPT = `({
  request: {
    url: "{{baseUrl}}/api/user/usage",
    method: "GET",
    headers: { "Authorization": "Bearer {{apiKey}}" }
  },
  extractor: function (response) {
    var d = typeof response === "string" ? JSON.parse(response) : response;
    var isCredits = d.limitUnit === "credits";
    var used = isCredits ? (d.totalCredits || 0) : (d.totalCost || 0);
    var limit = d.spendingLimit;
    return {
      isValid: true,
      used: used,
      total: limit == null ? null : limit,
      remaining: limit == null ? null : Math.max(0, limit - used),
      unit: isCredits ? "credits" : "USD"
    };
  }
})`

/** UTF-8 安全的 base64：btoa 只吃 Latin-1，非 ASCII 字符会直接抛异常 */
function utf8ToB64(text: string): string {
  const bytes = new TextEncoder().encode(text)
  return btoa(Array.from(bytes, (byte) => String.fromCharCode(byte)).join(''))
}

export interface CcSwitchTarget {
  apiKey: string
  /** API Key 名称（订单编号），用于区分 cc-switch 里的多张卡片 */
  keyName?: string
}

/** 生成 cc-switch 深链接。Base URL 取当前页面 origin */
export function buildCcSwitchDeeplink(app: CcSwitchApp, target: CcSwitchTarget): string {
  const origin = window.location.origin
  const keyName = target.keyName?.trim()
  const name = keyName ? `kiro2cc · ${keyName}` : 'kiro2cc'

  const params = new URLSearchParams()
  params.set('resource', 'provider')
  params.set('app', app)
  params.set('name', name)
  params.set('homepage', origin)
  // Codex 走 OpenAI Responses 协议，base_url 需带 /v1；Claude Code 会自己拼 /v1
  params.set('endpoint', app === 'codex' ? `${origin}/v1` : origin)
  params.set('apiKey', target.apiKey)
  // Claude 侧不指定模型：转换层的模型映射是模糊匹配，客户端默认模型名都能落到上游
  if (app === 'codex') params.set('model', CODEX_MODEL)
  params.set('usageEnabled', 'true')
  params.set('usageScript', utf8ToB64(USAGE_SCRIPT))
  // 必须显式给 usageBaseUrl：省略时 cc-switch 会回退到 endpoint，
  // Codex 侧那是 origin/v1，脚本 URL 会变成 origin/v1/api/user/usage
  params.set('usageBaseUrl', origin)
  params.set('usageAutoInterval', USAGE_AUTO_INTERVAL)
  // 不传 enabled：切到第三方 Codex 供应商会删掉用户的 ~/.codex/auth.json，交给用户自己决定

  return `ccswitch://v1/import?${params.toString()}`
}

/**
 * 跳转到 cc-switch 完成导入。
 *
 * 协议未注册时浏览器不会离开当前页，用「短暂延迟后仍持有焦点」判定未安装。
 * 这是启发式判断，不保证准确，所以失败路径只提示并给出可复制的链接。
 */
export function importToCcSwitch(
  app: CcSwitchApp,
  target: CcSwitchTarget,
  handlers: { onNotInstalled: (link: string) => void }
): void {
  const link = buildCcSwitchDeeplink(app, target)
  try {
    window.open(link, '_self')
    window.setTimeout(() => {
      if (document.hasFocus()) handlers.onNotInstalled(link)
    }, 100)
  } catch {
    handlers.onNotInstalled(link)
  }
}
