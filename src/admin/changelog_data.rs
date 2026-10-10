// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 更新日志静态数据
//!
//! 与 `build_model_list()`（`src/anthropic/handlers.rs`）同模式：编译期硬编码，
//! 随代码发布同步维护。新增版本时在 `build_release_notes()` 顶部追加一条，
//! 并将上一条的 `is_latest` 改回 `false`。

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
            version: "3.4.20".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "effort 徽章与深度思考状态联动入库：修复已关闭深度思考的账号用量记录仍展示 effort 徽章的问题；修复 sonnet-4.5 等走文本标签协议的模型开启深度思考后不展示 effort 的问题（thinking 生效时未携带 effort 自动补记，未生效一律丢弃）",
                "Effort badges in usage records now follow the effective thinking decision: fixed accounts with thinking disabled still showing effort badges, and fixed text-label-protocol models like sonnet-4.5 not showing any effort badge even with thinking enabled (effort is now backfilled when thinking is effective and dropped otherwise)",
            )])],
        },
        ReleaseNote {
            version: "3.4.19".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 issue #47 残留漂移：CC 每轮追加的 <total_tokens>N tokens left</total_tokens> token 预算提示导致 history[0] 冻结 key 仍逐轮 +50 字节漂移——新增 is_token_budget_reminder 谓词整块识别该消息，与 hook 动态块同样分流到当前消息开头的 <system-reminder>，模型仍可见但不参与缓存哈希",
                "Fixes the residual drift of issue #47: the per-turn <total_tokens>N tokens left</total_tokens> token budget reminder appended by CC still drifted the history[0] freeze key by 50 bytes per turn — a new is_token_budget_reminder predicate now recognizes it whole-block and, like hook dynamic blocks, splits it into a leading <system-reminder> of the current message, visible to the model but excluded from the cache hash",
            )])],
        },
        ReleaseNote {
            version: "3.4.18".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "账号级深度思考开关升级为最终裁决：关闭时剥离请求中的 thinking 控制与响应中的推理内容（强制关），开启时对支持的模型强制注入 thinking 配置（强制开），故障转移切换账号后按实际生效账号重新计算",
                    "The account-level deep-thinking switch becomes the final arbiter: off strips thinking controls from requests and reasoning from responses (forced off), on force-injects thinking config for supported models (forced on), recomputed after account failover based on the effective account",
                )]),
                fix_group(vec![
                    Bilingual::new(
                        "修复 issue #47：CC 注入的动态系统块（hook 输出、工具/MCP 通知）逐轮累积导致 history[0] 冻结缓存 key 漂移、前缀缓存持续失效——现分流到当前消息开头的 <system-reminder>，模型仍可见但不参与缓存哈希，对全部模型生效",
                        "Fixes issue #47: dynamic system blocks injected by CC (hook outputs, tool/MCP notices) accumulated per turn, drifting the history[0] freeze-cache key and continuously invalidating the prefix cache — they are now split into a leading <system-reminder> of the current message, visible to the model but excluded from the cache hash, for all models",
                    ),
                    Bilingual::new(
                        "thinkingAsText 渲染的文本化思考块回传上游时整块丢弃（标记行 + 思考正文 + 时长行），上游上下文不再含思考内容；流中断缺时长行时保守降级为只剥标记行",
                        "Thinking-as-text blocks are dropped entirely when sent upstream (marker line + thinking body + duration line), so the upstream context no longer contains thinking; conservatively falls back to stripping only the marker line when the duration line is missing due to a stream interruption",
                    ),
                    Bilingual::new(
                        "原生思考支持收紧为仅 claude-* 模型，修复 minimax-m2.1 / auto 等非 Claude 模型被强制注入 thinking 配置导致上游 400",
                        "Native thinking support is tightened to claude-* models only, fixing upstream 400s caused by force-injecting thinking config into non-Claude models such as minimax-m2.1 / auto",
                    ),
                    Bilingual::new(
                        "连续纯思考 assistant 消息合并后 content 为空违反 Kiro 非空约束的问题补占位符修复",
                        "Adds placeholder fix for empty content after merging consecutive thinking-only assistant messages, which violated Kiro's non-empty constraint",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.16".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "支持 claude-haiku-5.5 模型映射：1M 窗口 / 128K max output，定价 $0.10/$0.50，输入超 100K 整档跳价 $0.50/$2.50（不加入 /v1/models 模型列表）",
                "Adds claude-haiku-5.5 model mapping: 1M context / 128K max output, priced at $0.10/$0.50, tiering up to $0.50/$2.50 when input exceeds 100K (not listed in /v1/models)",
            )])],
        },
        ReleaseNote {
            version: "3.4.15".to_string(),
            is_latest: false,
            groups: vec![
                improve_group(vec![Bilingual::new(
                    "请求带 Write 工具时，在系统提示末尾追加分块写入约束：上游生成大块工具参数期间不发送数据，静默约 240s 后会重置流，引导模型把大文件拆成多次较小的写入以避免该重置（对客户端规则遵从度的影响尚未验证）",
                    "When a request carries a Write tool, a chunked-writing instruction is appended to the end of the system prompt: the upstream sends no data while a large tool argument is being generated and resets the stream after about 240s of silence, so the model is guided to split big files into several smaller writes (impact on adherence to client rules not yet verified)",
                )]),
                fix_group(vec![Bilingual::new(
                    "流中断提示不再断言「上游单次流时长上限」等未证实的原因，仅附带流已持续秒数",
                    "The stream-interruption message no longer asserts unverified causes such as an upstream per-stream duration limit and only includes the elapsed seconds",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.14".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "思考文本化时长行追加思考 token 统计（如「Thought for 22s (1.23k tokens)」），历史剥离兼容无后缀的旧格式",
                    "Thinking-as-text duration line now appends the thinking token count (e.g. \"Thought for 22s (1.23k tokens)\"); history stripping stays compatible with the old suffix-less format",
                )]),
                improve_group(vec![Bilingual::new(
                    "上游中断已产出部分内容的流时，日志新增 stream_elapsed_secs，返回客户端的错误信息附带流已持续的秒数，便于排查",
                    "When the upstream interrupts a stream that already produced output, the log now records stream_elapsed_secs and the error sent to the client includes the elapsed seconds, to aid troubleshooting",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.13".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "余额弹窗重设计：新增账号身份、认证与区域、运行健康三组信息卡片，展示登录来源、认证方式、Auth/API Region、代理与自适应注入状态及调用指标",
                        "Balance dialog redesigned with identity, auth & region, and runtime health cards showing login source, auth method, Auth/API regions, proxy and adaptive injection status, plus call metrics",
                    ),
                    Bilingual::new(
                        "登录来源支持在新增/编辑账号与批量导入时录入（仅 Google / GitHub，服务端无法从令牌推断）",
                        "Login source can now be set when adding/editing accounts and during batch import (Google / GitHub only; the server cannot infer it from the token)",
                    ),
                    Bilingual::new(
                        "账号级 thinking adaptive 开关默认开启，存量账号缺失该字段时同样视为开启",
                        "Per-account thinking adaptive now defaults to on; existing accounts missing the field are also treated as on",
                    ),
                    Bilingual::new(
                        "思考文本化收尾追加「Thought for Ns」时长行，历史剥离同步支持新旧两种格式",
                        "Thinking-as-text output now appends a \"Thought for Ns\" duration line, with history stripping supporting both old and new formats",
                    ),
                ]),
                improve_group(vec![Bilingual::new(
                    "思考文本化时长行 ≥60s 改为分秒格式（如 1m15s）",
                    "Thinking-as-text duration line now uses minute-second format for ≥60s (e.g. 1m15s)",
                )]),
                fix_group(vec![
                    Bilingual::new(
                        "管理台侧边栏语言徽标改为显示当前语言（中文显示「中」，英文显示 EN）",
                        "Admin sidebar language badge now shows the current language (中 for Chinese, EN for English)",
                    ),
                    Bilingual::new(
                        "移除设置页重复的侧栏收起开关项，保留侧栏折叠按钮",
                        "Removed the duplicate sidebar collapse toggle from settings, keeping the sidebar collapse button",
                    ),
                    Bilingual::new(
                        "修改登录来源不再清空账号订阅等级",
                        "Changing the login source no longer clears the account subscription tier",
                    ),
                    Bilingual::new(
                        "清空账号级代理地址时同步清除代理认证信息，避免残留无地址的凭据",
                        "Clearing a per-account proxy URL now also clears its credentials, avoiding orphaned auth data",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.12".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "思考文本化渲染去掉每行引用前缀，消除客户端显示的竖线",
                "Thinking-as-text rendering no longer prefixes each line with quote markers, removing the vertical bar shown by clients",
            )])],
        },
        ReleaseNote {
            version: "3.4.11".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "「思考内容文本化展示」默认开启：新用户搭建后即默认生效，无需手动开启",
                "Show Thinking as Text is now enabled by default: new deployments get it out of the box without manual toggling",
            )])],
        },
        ReleaseNote {
            version: "3.4.10".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复上游偶发回显 Claude Code system-reminder 元提示，避免该文本泄漏到客户端响应正文",
                "Fixed occasional upstream echoing of Claude Code system-reminder metadata, preventing it from leaking into client response bodies",
            )])],
        },
        ReleaseNote {
            version: "3.4.9".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "新增 thinkingAsText 思考内容文本化展示，设置页支持热切换；固定使用 ANSI dim 变暗样式",
                    "Adds thinkingAsText mode rendering thinking content as text with ANSI dim styling; hot-toggle in settings page",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复 GPT 系动态 hook 块分流破坏 history[0] 前缀缓存稳定性的问题",
                    "Fixed GPT-series dynamic hook block routing breaking history[0] prefix cache stability",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.8".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 v3.4.1 起客户端规则（CLAUDE.md / system-reminder）遵从性回退：原生 thinking 字段仅对 adaptive 请求注入，enabled 请求不再叠加",
                "Fixed client rule compliance (CLAUDE.md / system-reminder) regression since v3.4.1: native thinking field is now injected only for adaptive requests, no longer stacked on enabled requests",
            )])],
        },
        ReleaseNote {
            version: "3.4.7".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "设置页新增 Suggestion Mode（输入建议拦截）与客户端 token 直通热切换开关",
                        "Settings page adds hot-toggle switches for Suggestion Mode interception and client token passthrough",
                    ),
                    Bilingual::new(
                        "设置页新增 maxRPM、端口、代理地址三项运行时配置，免重启即时生效",
                        "Settings page adds maxRPM, port, and proxy URL runtime config, taking effect without restart",
                    ),
                ]),
                fix_group(vec![
                    Bilingual::new(
                        "修复配置文件原子替换丢失原文件权限、三处热更新与磁盘写入顺序不一致的问题",
                        "Fixed atomic config replacement losing original file permissions and three hot-update ordering inconsistencies with disk writes",
                    ),
                    Bilingual::new(
                        "修复建议模式识别可能误吞正常多块消息的问题",
                        "Fixed suggestion-mode detection potentially swallowing normal multi-block messages",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.6".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "Claude 系模型支持注入 thinking 字段（enable-thinking-display 配置开关），客户端可观测深度思考输出",
                        "Claude-family models can inject the thinking field via the enable-thinking-display toggle, making reasoning output observable to clients",
                    ),
                    Bilingual::new(
                        "新增请求级深度思考状态日志：每条请求同时记录客户端请求的 thinking 配置与实际生效的注入结果（request/effective 双维度）",
                        "Added request-level thinking status logs: each request records both the client-requested thinking config and the actually applied injection (request/effective dimensions)",
                    ),
                    Bilingual::new(
                        "账号表新增多列排序（请求数/错误率/额度等）并持久化排序状态；Base URL 卡片新增一键复制",
                        "Account table gains multi-column sorting (requests, error rate, balance, etc.) with persisted sort state; the Base URL card adds one-click copy",
                    ),
                ]),
                improve_group(vec![Bilingual::new(
                    "实时日志页加载与渲染性能优化：日志合批刷新 + 行按视口虚拟化，长日志洪峰下页面不再卡顿",
                    "Realtime logs page load and rendering performance optimization: batched log refresh plus viewport-virtualized rows keep the page responsive under heavy log floods",
                )]),
                fix_group(vec![
                    Bilingual::new(
                        "修复 API Key 请求次数超过 1 万次后统计封顶不再增长的问题：明细裁剪部分累计进持久化基数，请求数与 credits 持续准确累计，重启不丢",
                        "Fixed API Key request counts capping at 10,000: pruned detail records are accumulated into a persisted lifetime base, so request counts and credits keep growing accurately across restarts",
                    ),
                    Bilingual::new(
                        "第三方模型（GPT 系等非 Claude 家族）跳过 additionalModelRequestFields，修复该类请求 400 错误",
                        "Third-party models (GPT and other non-Claude families) now skip additionalModelRequestFields, fixing 400 errors for such requests",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.3.2".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "新增账号级深度思考（thinking adaptive）注入开关：管理界面每个账号可独立开关。开启后，客户端请求携带 thinking: {type: 'adaptive'} 且目标模型支持时，代理在发送给 Kiro 前向 additionalModelRequestFields 注入 thinking 字段；关闭时完全透传不注入。开关立即生效（无需重启）并持久化到 credentials.json。旧版配置文件无该字段时默认关闭，兼容无害",
                "Added a per-account thinking adaptive injection toggle: each account can be toggled independently in the admin UI. When enabled, if the client request carries thinking: {type: 'adaptive'} and the target model supports it, the proxy injects the thinking field into additionalModelRequestFields before sending to Kiro; when disabled, requests are passed through without injection. The toggle takes effect immediately (no restart needed) and is persisted to credentials.json. Legacy config files without the field default to disabled, fully compatible",
            )])],
        },
        ReleaseNote {
            version: "3.3.1".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 BuilderId（含教育版）账号无法查询额度、添加报 400 Invalid profileArn 的问题：上游数据面对缺失 profileArn 的请求直接拒绝，此类账号（部分 BuilderId/教育账号天然无 profileArn）导入或添加后对话与额度查询均失败。现按 Kiro IDE 行为在账号加载/添加时自动注入固定 fallback ARN 并持久化存储，数据面请求 400 提示 profileArn is required 时首即禁用该账号并自动切换到可用账号，不再影响其他账号的正常使用",
                "Fixed BuilderId (including education) accounts failing balance queries with 400 Invalid profileArn: the upstream data plane rejects requests missing profileArn, so such accounts (some BuilderId/education accounts naturally have none) failed both conversations and balance queries after import or addition. Following Kiro IDE behaviour, a fixed fallback ARN is now injected automatically at account load/add time and persisted; when a data-plane request returns 400 profileArn is required, the account is disabled immediately and traffic fails over to other available accounts",
            )])],
        },
        ReleaseNote {
            version: "3.3.0".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复反代链路 TTFB 显著慢于 Kiro CLI 直连的问题：代理此前会将 thinking: {type: 'adaptive'} 写入 additionalModelRequestFields 并向 system 消息注入 <thinking_mode>adaptive</thinking_mode> 标签，触发 Kiro 后端额外的 thinking 调度路径，导致首包延迟（TTFB）增加 2–4 秒。对齐 Kiro CLI 行为：不发 thinking 字段，让 Kiro 后端走默认路径。影响模型：claude-sonnet-4.6 / opus-4.6 / opus-4.7 / opus-4.8 / sonnet-5 / opus-5",
                "Fixed TTFB significantly higher through the proxy than with direct Kiro CLI connections: the proxy was injecting thinking: {type: 'adaptive'} into additionalModelRequestFields and adding <thinking_mode>adaptive</thinking_mode> to the system prompt, triggering an extra thinking scheduling path on the Kiro backend and adding 2–4 s to the first-byte latency. Aligned with Kiro CLI behaviour: no longer sending the thinking field, letting the Kiro backend use its default path. Affected models: claude-sonnet-4.6 / opus-4.6 / opus-4.7 / opus-4.8 / sonnet-5 / opus-5",
            )])],
        },
        ReleaseNote {
            version: "3.2.1".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "additionalModelRequestFields 中的 effort 改为仅透传：客户端携带 output_config 时按原值转发，未携带时不再默认注入 effort=\"high\"，修复反代链路 claude-sonnet-5 / claude-opus-5 响应显著慢于 Kiro IDE 直连的问题",
                "The effort in additionalModelRequestFields is now pass-through only: it is forwarded as-is when the client sends output_config and no longer defaults to effort=\"high\" when absent, fixing claude-sonnet-5 / claude-opus-5 responding much slower through the proxy than direct Kiro IDE connections",
            )])],
        },
        ReleaseNote {
            version: "3.2.0".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "WebSearch 工具桥接升级为 Kiro MCP 真实搜索闭环：客户端 web_search 工具声明被转换为 Kiro 侧执行，多轮工具调用自动衔接，支持 max_uses 上限控制",
                    "WebSearch tool bridging upgraded to a real Kiro MCP search loop: client web_search tool declarations are executed on the Kiro side with automatic multi-round tool-call handoff and max_uses capping",
                )]),
                improve_group(vec![Bilingual::new(
                    "桥接轮次保活机制重构：后台搜索轮次经 select! 条件分支收割，避免流式循环提前退出导致搜索结果丢失",
                    "Bridge keep-alive reworked: background search rounds are harvested via a select! conditional branch, preventing search results from being lost when the streaming loop exits early",
                )]),
                fix_group(vec![Bilingual::new(
                    "恢复 4.5 代际模型（haiku-4-5 / sonnet-4-5 / opus-4-5 及其带日期变体）跳过 additionalModelRequestFields，修复该代际全量请求 400 REQUEST_BODY_INVALID / 502",
                    "Restore the 4.5 generation models (haiku-4-5 / sonnet-4-5 / opus-4-5 and their dated variants) skipping additionalModelRequestFields, fixing widespread 400 REQUEST_BODY_INVALID / 502 failures for that generation",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.0.17".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "凭据管理支持从 cc-switch 导入账号配置",
                    "Credential management can now import account configurations from cc-switch",
                )]),
                improve_group(vec![Bilingual::new(
                    "流式与非流式路径的 [TOOLUSE-DIAG] 诊断日志改为仅在检测到结构异常时告警，正常响应降为 debug，实时日志页的告警指标不再被每个请求刷屏",
                    "The [TOOLUSE-DIAG] diagnostic on both streaming and non-streaming paths now warns only on detected structural anomalies and drops to debug otherwise, so the realtime log page's warning metrics are no longer flooded by every request",
                )]),
                fix_group(vec![
                    Bilingual::new(
                        "新增账号不再复用已删除账号的 ID，避免在管理面板中继承该 ID 下的历史用量与限流记录",
                        "New accounts no longer reuse IDs from deleted accounts, preventing them from inheriting that ID's historical usage and throttling records in the admin panel",
                    ),
                    Bilingual::new(
                        "ID 计数器落盘改为在阻塞线程池执行并按最大值合并写入，消除并发分配时的覆盖竞态",
                        "The ID counter now persists on the blocking thread pool and merges by maximum value, eliminating the overwrite race during concurrent allocation",
                    ),
                    Bilingual::new(
                        "本地启动脚本改为清理全部占用端口的进程并轮询等待端口释放，修复端口占用竞态导致的启动失败",
                        "The local startup script now clears every process holding the port and polls until it is released, fixing startup failures caused by the port-in-use race",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.0.1".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "新增 OpenAI 兼容端点 /v1/chat/completions 与 /v1/responses，支持 Codex CLI 与 OpenAI SDK 类客户端直接接入，用 Kiro 额度调用 GPT-5.6 系列模型",
                    "Added OpenAI-compatible endpoints /v1/chat/completions and /v1/responses, letting Codex CLI and OpenAI SDK clients connect directly and run GPT-5.6 models on Kiro credits",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复 OpenAI 兼容端点流式转换中孤儿 tool_call 与 SSE 溢出日志的问题",
                    "Fixed orphaned tool_call handling and SSE overflow logging in the OpenAI-compatible streaming conversion",
                )]),
            ],
        },
        ReleaseNote {
            version: "2.10.2".to_string(),
            is_latest: false,
            groups: vec![
                improve_group(vec![
                    Bilingual::new(
                        "/cc/v1/messages 改为实时流式转发，删除整段缓冲，首字延迟不再等待上游响应结束",
                        "/cc/v1/messages now forwards the stream in real time; the full-response buffer is gone, so time-to-first-token no longer waits for the upstream to finish",
                    ),
                    Bilingual::new(
                        "Admin/User 前端图标改用 Aurora Prism 方案，侧边栏 logo 随明暗主题切换",
                        "Admin/User front-end icons switched to the Aurora Prism set; the sidebar logo follows the light/dark theme",
                    ),
                ]),
                fix_group(vec![
                    Bilingual::new(
                        "上报给客户端的 output_tokens 解除 380 固定上限，并按来源排除 thinking 内容",
                        "Client-facing output_tokens no longer capped at 380, and thinking content is excluded by source rather than by cap",
                    ),
                    Bilingual::new(
                        "输出 token 估算改为累加字符数后统一取整，消除逐 chunk 向上取整的累积高估",
                        "Output token estimation accumulates characters and rounds once at the end, removing the systematic overestimate from per-chunk rounding",
                    ),
                    Bilingual::new(
                        "上下文超窗错误改用 Anthropic 官方文案格式，便于客户端识别并触发自动压缩重试",
                        "Context-overflow errors now use Anthropic's official message format so clients can recognize them and trigger auto-compaction retries",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "2.10.0".to_string(),
            is_latest: false,
            groups: vec![
                improve_group(vec![
                    Bilingual::new(
                        "Admin 后台全站 12 个页面按新设计稿改版，账号管理与 API Keys 改为表格化布局",
                        "All 12 Admin console pages redesigned to the new visual spec; accounts and API Keys switched to a table layout",
                    ),
                    Bilingual::new(
                        "实时日志页新增缓冲区/错误/警告指标卡、级别分段筛选与重复行折叠",
                        "Realtime logs page adds buffer/error/warning metric cards, level segment filters, and duplicate-row collapsing",
                    ),
                    Bilingual::new(
                        "每日统计页支持 7/14/30 天区间切换，趋势图仅标注峰值",
                        "Daily stats page supports 7/14/30-day range switching; the trend chart annotates peaks only",
                    ),
                ]),
                fix_group(vec![
                    Bilingual::new(
                        "每日统计「今天」改按 CST(UTC+8) 计算，与后端聚合口径对齐",
                        "Daily stats now computes \"today\" in CST (UTC+8) to match the backend aggregation window",
                    ),
                    Bilingual::new(
                        "实时日志时间戳按浏览器时区渲染，不再把 UTC 时钟当本地时间显示",
                        "Realtime log timestamps render in the browser time zone instead of showing the UTC clock as local time",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "2.9.5".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "Admin 后台侧边栏支持折叠/展开",
                "Admin console sidebar now supports collapse/expand",
            )])],
        },
        ReleaseNote {
            version: "2.9.0".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "Admin 后台支持中英文全局切换",
                    "Admin console now supports global zh/en language switching",
                )]),
                improve_group(vec![Bilingual::new(
                    "设置页面重构为分组列表布局",
                    "Settings page redesigned with a grouped list layout",
                )]),
            ],
        },
        ReleaseNote {
            version: "2.8.25".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "支持模型列表按模型家族分组排列",
                "Supported models list is now grouped by model family",
            )])],
        },
        ReleaseNote {
            version: "2.8.24".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![
                Bilingual::new(
                    "Admin 登录密码字段由 adminApiKey 改名为 adminPsw",
                    "Renamed the admin login password field from adminApiKey to adminPsw",
                ),
                Bilingual::new(
                    "控制台标题链接新增 hover 高亮效果",
                    "Added a hover highlight effect to the console title link",
                ),
            ])],
        },
        ReleaseNote {
            version: "2.8.23".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "移除主 API Key 全局兜底认证机制，收窄鉴权入口",
                "Removed the global fallback authentication via the master API key to narrow the authentication surface",
            )])],
        },
        ReleaseNote {
            version: "2.8.22".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "支持模型页标记同家族内最低/最高费率模型",
                    "The supported models page now flags the lowest/highest priced model within each family",
                ),
                Bilingual::new(
                    "支持模型页按提供方家族着色",
                    "The supported models page is now color-coded by provider family",
                ),
                Bilingual::new(
                    "按模型分组卡片新增 credits 消费统计",
                    "Added credits consumption stats to model-grouped cards",
                ),
            ])],
        },
        ReleaseNote {
            version: "2.8.21".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "每日统计页新增最近 14 天 credits 使用趋势曲线图",
                "Added a 14-day credits usage trend chart to the daily stats page",
            )])],
        },
        ReleaseNote {
            version: "2.8.20".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Dockerfile 缺少 COPY assets 导致容器内 ip2region xdb 缺失、构建失败的问题",
                "Fixed a build failure caused by the Dockerfile missing a COPY assets step, which left the ip2region xdb file absent in the container",
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
