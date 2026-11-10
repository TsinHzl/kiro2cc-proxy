// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 更新日志静态数据
//!
//! 与 `build_model_list()`（`src/anthropic/handlers.rs`）同模式：编译期硬编码，
//! 随代码发布同步维护。新增版本时在 `build_release_notes()` 顶部追加一条即可，
//! `is_latest` 由 `CURRENT_VERSION` 与条目 `version` 自动比对设定，无需手工修改。
//!
//! 只收录改动大、用户能直接感知的重点变更；样式微调、文案润色与内部诊断日志不列入。

/// 中英双语文案
#[derive(Debug, Clone)]
pub struct Bilingual {
    pub zh: String,
    pub en: String,
}

impl Bilingual {
    fn new(zh: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            zh: zh.into(),
            en: en.into(),
        }
    }
}

/// 更新日志分类分组（固定使用「新功能」「优化」「修复」三类）
#[derive(Debug, Clone)]
pub struct ReleaseNoteGroup {
    pub title: Bilingual,
    pub items: Vec<Bilingual>,
}

/// 单个版本的更新日志
#[derive(Debug, Clone)]
pub struct ReleaseNote {
    pub version: String,
    pub is_latest: bool,
    pub groups: Vec<ReleaseNoteGroup>,
}

fn feat_group(items: Vec<Bilingual>) -> ReleaseNoteGroup {
    ReleaseNoteGroup {
        title: Bilingual::new("新功能", "New Features"),
        items,
    }
}

fn improve_group(items: Vec<Bilingual>) -> ReleaseNoteGroup {
    ReleaseNoteGroup {
        title: Bilingual::new("优化", "Improvements"),
        items,
    }
}

fn fix_group(items: Vec<Bilingual>) -> ReleaseNoteGroup {
    ReleaseNoteGroup {
        title: Bilingual::new("修复", "Fixes"),
        items,
    }
}

/// 当前发布版本（来自 Cargo.toml 的 [package].version 字段），编译期常量
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 构建更新日志列表，固定按版本号从新到旧声明（不做运行时排序）
///
/// `is_latest` 由 `CURRENT_VERSION` 与条目 `version` 比对自动设定，
/// bump Cargo.toml 后无需手动改此标记。
pub fn build_release_notes() -> Vec<ReleaseNote> {
    let mut notes = vec![
        ReleaseNote {
            version: "3.5.5".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![
                Bilingual::new(
                    "修复统计数据随明细条数上限裁剪而倒退的问题（issue #67）：日报、账号汇总及 Key 累计费用改读不裁剪的按日汇总，升级后首次启动自动回填，升级前已裁剪的历史无法恢复",
                    "Fixed statistics shrinking when detail records were pruned (issue #67): daily reports, account totals and key cumulative cost now read a never-pruned daily aggregate, backfilled on first start after upgrade; history pruned before the upgrade cannot be recovered",
                ),
                Bilingual::new(
                    "修复会话页历史消息中 thinking 标签泄漏的问题",
                    "Fixed thinking tags leaking into chat page history messages",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.5.4".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 OpenAI Responses 端点中孤立的 function_call_output 导致会话持续 400 的问题（issue #66）",
                "Fixed orphaned function_call_output items in the OpenAI Responses endpoint causing every later request in a session to fail with 400 (issue #66)",
            )])],
        },
        ReleaseNote {
            version: "3.5.3".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "对话页新增后端存储的历史会话，支持选择 API Key、模型与思考力度，并支持联网搜索与图片/文本附件",
                    "The chat page now keeps persistent sessions on the backend, with API key, model and thinking-effort selection, web search, and image / text-file attachments",
                ),
                Bilingual::new(
                    "新增账号保活后台任务",
                    "Added a background account keep-alive task",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.5.2".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "客户端 token 直通（clientTokenPassthrough）默认开启，返回给客户端的 usage 为真实值；如需按系数缩放展示，可在设置中关闭",
                "Client token passthrough (clientTokenPassthrough) is now on by default, so usage returned to clients carries real values; turn it off to restore scaled display",
            )])],
        },
        ReleaseNote {
            version: "3.5.1".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "支持 Claude Fable 5.1（#60）；该模型仅对 Enterprise 客户开放，能否使用取决于账号权限",
                "Added Claude Fable 5.1 support (#60); Kiro only offers it to Enterprise customers, so availability depends on account access",
            )])],
        },
        ReleaseNote {
            version: "3.5.0".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Claude Code 反复出现「Resume directly」死循环的问题（#59）",
                "Fixed the endless Resume directly loop in Claude Code (#59)",
            )])],
        },
        ReleaseNote {
            version: "3.4.46".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "账号余额弹窗展示 Kiro 账号完整详情，包括配额分解、奖励明细、订阅信息、Token 到期与账号添加日期",
                "The account balance dialog now shows full Kiro account details, including quota breakdown, bonus details, subscription info, token expiry and the account added date",
            )])],
        },
        ReleaseNote {
            version: "3.4.45".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Sonnet 5.5 等模型的缓存读取几乎恒为 0 的问题：缓存读取反推改用全部 Claude 模型的实测费率",
                "Fixed cache reads being almost always 0 on models such as Sonnet 5.5, by using measured rates for all Claude models in cache-read inference",
            )])],
        },
        ReleaseNote {
            version: "3.4.43".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "聊天回复末尾新增本轮与会话累计 credits 展示（showSessionCredits）",
                "Chat replies now end with turn and session credits (showSessionCredits)",
            )])],
        },
        ReleaseNote {
            version: "3.4.42".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "账号管理与使用日志页切回浏览器标签页时自动刷新数据",
                "The account manager and usage log pages now refresh data when the browser tab regains focus",
            )])],
        },
        ReleaseNote {
            version: "3.4.41".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "设置页的 Kiro 请求体落盘取证支持网页浏览与下载（单个或 ZIP 打包），容器部署无需 SSH 取回",
                    "The Kiro request-body dump setting can now browse and download dump files in the web UI, individually or as a ZIP, so containerised deployments need no SSH",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复编辑账号时 Client Secret 被浏览器自动填充的管理密码覆盖的问题（#58）；已被覆盖的账号需重新填入真实的 clientSecret",
                    "Fixed the Client Secret being overwritten by an autofilled admin password when editing an account (#58); affected accounts must re-enter their real clientSecret",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.40".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "输出 Tokens 改用 tiktoken BPE 真值统计，与思考 Token 口径一致，修复总输出少于思考 Token 的矛盾",
                    "Output tokens are now counted with tiktoken BPE, matching the thinking token count, which fixes total output appearing smaller than thinking tokens",
                ),
                Bilingual::new(
                    "根据上游 credits 消耗反推缓存读取 Token 数，缓存读取首次可观测",
                    "Cache-read tokens are now inferred from upstream credit consumption, making cache reads observable for the first time",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.39".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Claude Code 长会话中任务提醒使上游前缀缓存整段失效、credits 偏高的问题",
                "Fixed the mid-session task reminder in long Claude Code sessions invalidating the whole upstream prefix cache and inflating credits",
            )])],
        },
        ReleaseNote {
            version: "3.4.38".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Codex Desktop 通过 /v1/responses 接入时 MCP 工具与子代理工具全部不可用的问题（#57）",
                "Fixed MCP and sub-agent tools being unavailable when Codex Desktop connects via /v1/responses (#57)",
            )])],
        },
        ReleaseNote {
            version: "3.4.37".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Claude Code 开启 advisor 后整个会话每次请求都返回 400 的问题（#56）",
                "Fixed HTTP 400 on every request once Claude Code enables advisor (#56)",
            )])],
        },
        ReleaseNote {
            version: "3.4.36".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Claude Desktop Cowork 模式下思考内容出现 ESC 方块与 ANSI 转义乱码的问题（#54）",
                "Fixed ESC blocks and literal ANSI sequences appearing in thinking content in Claude Desktop Cowork mode (#54)",
            )])],
        },
        ReleaseNote {
            version: "3.4.35".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "账号余额弹窗新增今日、近 7 天、近 30 天的多时段用量统计",
                "The account balance dialog adds usage stats for today, the last 7 days and the last 30 days",
            )])],
        },
        ReleaseNote {
            version: "3.4.34".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "账号编辑弹窗重构：已有凭据改为只读锁定展示，无值字段仍可补录",
                    "Account edit dialog redesigned: existing credentials are shown read-only, while empty fields stay editable",
                ),
                Bilingual::new(
                    "使用日志与账号详情页新增总费用、总 Credits 与总 Token 统计",
                    "Usage log and account detail pages add total cost, total credits and total token stats",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.33".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "admin-ui 新增「使用文档」页，内置快速接入、开发指南、接口参考与排障指南",
                "New Usage Docs page in admin-ui with quick start, development guide, API reference and troubleshooting",
            )])],
        },
        ReleaseNote {
            version: "3.4.32".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "设置页新增 DEBUG 日志开关（debugLogs），可热切换，无需重启",
                    "New debugLogs switch in Settings to toggle DEBUG logs at runtime without a restart",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复 Claude Code Read 工具返回的图片被静默丢弃的问题（issue #52，感谢 @jayus71 的 PR #53）",
                    "Fixed images returned by Claude Code's Read tool being silently dropped (issue #52, thanks to @jayus71 for PR #53)",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.31".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "新增 promptCacheAccounting 开关，按 Anthropic cache_control 规则在本地模拟缓存记账，默认关闭",
                "New promptCacheAccounting switch simulates cache accounting locally following Anthropic cache_control rules; disabled by default",
            )])],
        },
        ReleaseNote {
            version: "3.4.30".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "请求明细页全面接入按模型分组的统计面板（账号详情、API Key 详情、使用日志、日明细）",
                    "Per-model grouped stats panel rolled out to all request-detail views (account detail, API key detail, usage log, daily detail)",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复 local-agent 等入口的思考文本化静默失效的问题（v3.4.23 引入的回归，issue #48）",
                    "Fixed thinking-as-text being silently disabled for local-agent and other entrypoints (regression from v3.4.23, issue #48)",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.27".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "支持上游新增的 kimi-k3 模型",
                    "Supports the new upstream kimi-k3 model",
                ),
                Bilingual::new(
                    "请求日志新增深度思考 Token 统计",
                    "Usage log adds thinking token statistics",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.26".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "admin-ui 新增主题设置页，提供 37 套色彩主题与明暗模式组合",
                "admin-ui adds a theme settings page with 37 color themes combined with light and dark modes",
            )])],
        },
        ReleaseNote {
            version: "3.4.25".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "混合账号池按账号实际支持的模型路由，不再把请求分发到不支持目标模型的账号（issue #49）",
                "Mixed account pools now route by each account's supported models, so requests are no longer sent to accounts that lack the target model (issue #49)",
            )])],
        },
        ReleaseNote {
            version: "3.4.24".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Claude Desktop 开启 thinkingAsText 时思考内容出现 ANSI 乱码的问题（issue #48）",
                "Fixed ANSI garbage in thinking output on Claude Desktop when thinkingAsText is enabled (issue #48)",
            )])],
        },
        ReleaseNote {
            version: "3.4.22".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "支持企业版 IdC 账号（IAM Identity Center），自动发现真实 profileArn（issue #43）",
                "Supports enterprise IdC accounts (IAM Identity Center) with automatic discovery of the real profileArn (issue #43)",
            )])],
        },
        ReleaseNote {
            version: "3.4.21".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "分块写入行数上限可配置，默认由 150 行放宽到 300 行（writeChunkingMaxLines，issue #46）",
                "The chunked-write line limit is now configurable and raised from 150 to 300 lines by default (writeChunkingMaxLines, issue #46)",
            )])],
        },
        ReleaseNote {
            version: "3.4.20".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 effort 徽章与深度思考生效状态不一致的问题",
                "Fixed effort badges not matching the effective thinking state",
            )])],
        },
        ReleaseNote {
            version: "3.4.19".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 issue #47 残留的缓存前缀漂移：token 预算提醒不再使 history[0] 缓存 key 逐轮变化",
                "Fixed the residual cache prefix drift in issue #47: the token budget reminder no longer changes the history[0] cache key every turn",
            )])],
        },
        ReleaseNote {
            version: "3.4.18".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "账号级深度思考开关成为最终裁决：关闭则强制关，开启则对支持的模型强制开，故障转移后按实际账号重算",
                    "The account-level deep-thinking switch is now final: off forces thinking off, on forces it on for supported models, recalculated after failover",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复 issue #47：CC 注入的动态系统块逐轮累积导致前缀缓存持续失效",
                    "Fixed issue #47: dynamic system blocks injected by CC accumulated every turn and kept invalidating the prefix cache",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.16".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "支持 claude-haiku-5.5 模型映射（1M 窗口，128K 最大输出）",
                "Adds claude-haiku-5.5 model mapping (1M context, 128K max output)",
            )])],
        },
        ReleaseNote {
            version: "3.4.15".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "请求带 Write 工具时追加分块写入约束，减少大块工具参数生成期间上游流被重置的情况",
                "Requests with a Write tool get a chunked-writing instruction, reducing upstream stream resets caused by large tool arguments",
            )])],
        },
        ReleaseNote {
            version: "3.4.14".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "思考文本化的时长行追加思考 Token 统计",
                "The thinking-as-text duration line now includes the thinking token count",
            )])],
        },
        ReleaseNote {
            version: "3.4.13".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "余额弹窗重设计，新增账号身份、认证与区域、运行健康等信息卡片",
                    "The balance dialog is redesigned with identity, auth and region, and runtime health cards",
                ),
                Bilingual::new(
                    "账号级 thinking adaptive 开关默认开启",
                    "The per-account thinking adaptive switch now defaults to on",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.12".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复思考文本化输出在客户端显示多余竖线的问题",
                "Fixed stray vertical bars shown by clients in thinking-as-text output",
            )])],
        },
        ReleaseNote {
            version: "3.4.11".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "「思考内容文本化展示」默认开启",
                "Show Thinking as Text is now on by default",
            )])],
        },
        ReleaseNote {
            version: "3.4.10".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复上游偶发回显 system-reminder 元提示并泄漏到客户端正文的问题",
                "Fixed occasional upstream echoes of system-reminder metadata leaking into client response bodies",
            )])],
        },
        ReleaseNote {
            version: "3.4.9".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "新增 thinkingAsText 思考内容文本化展示，设置页可热切换",
                "Adds thinkingAsText to render thinking content as text, hot-toggleable in settings",
            )])],
        },
        ReleaseNote {
            version: "3.4.8".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 v3.4.1 起客户端规则（CLAUDE.md / system-reminder）遵从性回退的问题",
                "Fixed the client rule compliance (CLAUDE.md / system-reminder) regression introduced in v3.4.1",
            )])],
        },
        ReleaseNote {
            version: "3.4.7".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "设置页新增 Suggestion Mode、客户端 token 直通，以及 maxRPM、端口、代理地址的热更新",
                "Settings page adds hot-toggles for Suggestion Mode and client token passthrough, plus maxRPM, port and proxy URL",
            )])],
        },
        ReleaseNote {
            version: "3.4.6".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "Claude 系模型支持注入 thinking 字段（enable-thinking-display），客户端可观测深度思考输出",
                    "Claude-family models can inject the thinking field (enable-thinking-display), making reasoning output visible to clients",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复 API Key 请求次数超过 1 万次后统计封顶的问题",
                    "Fixed API key request counts capping at 10,000",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.3.2".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "新增账号级深度思考（thinking adaptive）注入开关，可按账号独立开关",
                "Adds a per-account thinking adaptive injection switch that can be toggled independently for each account",
            )])],
        },
        ReleaseNote {
            version: "3.3.1".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 BuilderId 账号无法查询额度、报 400 Invalid profileArn 的问题",
                "Fixed BuilderId accounts failing balance queries with 400 Invalid profileArn",
            )])],
        },
        ReleaseNote {
            version: "3.3.0".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复反代链路首包延迟明显高于 Kiro CLI 直连的问题",
                "Fixed first-byte latency through the proxy being significantly higher than direct Kiro CLI connections",
            )])],
        },
        ReleaseNote {
            version: "3.2.1".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "effort 参数改为仅透传，修复 claude-sonnet-5 / claude-opus-5 反代链路响应明显变慢的问题",
                "effort is now pass-through only, fixing much slower responses from claude-sonnet-5 / claude-opus-5 through the proxy",
            )])],
        },
        ReleaseNote {
            version: "3.2.0".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "WebSearch 工具桥接升级为 Kiro MCP 真实搜索闭环",
                    "WebSearch tool bridging upgraded to a real Kiro MCP search loop",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复 4.5 代际模型（haiku-4-5 / sonnet-4-5 / opus-4-5）全量请求 400 / 502 的问题",
                    "Fixed widespread 400 / 502 failures for the 4.5 generation models (haiku-4-5 / sonnet-4-5 / opus-4-5)",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.0.17".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "新增账号不再复用已删除账号的 ID，避免继承历史用量与限流记录",
                "New accounts no longer reuse IDs from deleted accounts, avoiding inherited usage and throttling records",
            )])],
        },
        ReleaseNote {
            version: "3.0.1".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "新增 OpenAI 兼容端点 /v1/chat/completions 与 /v1/responses，可用 Kiro 额度调用 GPT-5.6 系列模型",
                "Adds OpenAI-compatible endpoints /v1/chat/completions and /v1/responses to run GPT-5.6 models on Kiro credits",
            )])],
        },
        ReleaseNote {
            version: "2.10.2".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "/cc/v1/messages 改为实时流式转发，首字延迟不再等待上游响应结束",
                "/cc/v1/messages now forwards the stream in real time, so time to first token no longer waits for the full upstream response",
            )])],
        },
        ReleaseNote {
            version: "2.10.0".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "Admin 后台 12 个页面按新设计稿改版",
                "All 12 Admin console pages redesigned to the new visual spec",
            )])],
        },
        ReleaseNote {
            version: "2.9.5".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "Admin 后台侧边栏支持折叠/展开",
                "Admin console sidebar supports collapse and expand",
            )])],
        },
        ReleaseNote {
            version: "2.9.0".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "Admin 后台支持中英文切换",
                "Admin console supports switching between Chinese and English",
            )])],
        },
        ReleaseNote {
            version: "2.8.25".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "模型列表按模型家族分组排列",
                "Supported models list is grouped by model family",
            )])],
        },
        ReleaseNote {
            version: "2.8.24".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "管理员登录密码字段由 adminApiKey 改名为 adminPsw",
                "Renamed the admin login password field from adminApiKey to adminPsw",
            )])],
        },
        ReleaseNote {
            version: "2.8.23".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "移除主 API Key 全局兜底认证，收窄鉴权入口",
                "Removed the global fallback authentication via the master API key, narrowing the authentication surface",
            )])],
        },
        ReleaseNote {
            version: "2.8.22".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "模型页按提供方家族着色，并标记同家族内最低与最高费率的模型",
                "The models page is color-coded by provider family and flags the lowest and highest priced model in each family",
            )])],
        },
        ReleaseNote {
            version: "2.8.21".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "每日统计页新增最近 14 天 credits 趋势图",
                "Daily stats page adds a 14-day credits trend chart",
            )])],
        },
        ReleaseNote {
            version: "2.8.20".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Docker 镜像缺少 ip2region 数据文件导致构建失败的问题",
                "Fixed a Docker build failure caused by the ip2region data file missing from the image",
            )])],
        },
    ];

    // is_latest 后处理：与 CURRENT_VERSION 匹配的条目标为最新，
    // 列表里找不到时全部置 false（避免历史版本继续被高亮）
    let matched = notes
        .iter()
        .filter(|n| n.version == CURRENT_VERSION)
        .count();
    if matched == 1 {
        for n in notes.iter_mut() {
            n.is_latest = n.version == CURRENT_VERSION;
        }
    } else {
        for n in notes.iter_mut() {
            n.is_latest = false;
        }
    }

    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exactly_one_latest_version() {
        let notes = build_release_notes();
        // 至多一条 is_latest=true（changelog 未列当前版本时全部 false）
        assert!(
            notes.iter().filter(|n| n.is_latest).count() <= 1,
            "is_latest=true 至多一条"
        );
        // 当前版本在 changelog 里时必须被标 latest
        if notes.iter().any(|n| n.version == CURRENT_VERSION) {
            assert!(
                notes
                    .iter()
                    .any(|n| n.version == CURRENT_VERSION && n.is_latest),
                "当前版本 {} 在 changelog 中时必须 is_latest=true",
                CURRENT_VERSION
            );
        }
    }

    #[test]
    fn test_version_format() {
        let notes = build_release_notes();
        for note in &notes {
            assert!(
                note.version.split('.').count() == 3
                    && note
                        .version
                        .split('.')
                        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())),
                "版本号格式非法: {}",
                note.version
            );
        }
    }

    #[test]
    fn test_bilingual_fields_non_empty() {
        let notes = build_release_notes();
        for note in &notes {
            for group in &note.groups {
                assert!(!group.title.zh.is_empty(), "分组标题 zh 不能为空");
                assert!(!group.title.en.is_empty(), "分组标题 en 不能为空");
                for item in &group.items {
                    assert!(!item.zh.is_empty(), "条目 zh 不能为空");
                    assert!(!item.en.is_empty(), "条目 en 不能为空");
                }
            }
        }
    }
}
