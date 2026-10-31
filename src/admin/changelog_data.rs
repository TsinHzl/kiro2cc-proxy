// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 更新日志静态数据
//!
//! 与 `build_model_list()`（`src/anthropic/handlers.rs`）同模式：编译期硬编码，
//! 随代码发布同步维护。新增版本时在 `build_release_notes()` 顶部追加一条即可，
//! `is_latest` 由 `CURRENT_VERSION` 与条目 `version` 自动比对设定，无需手工修改。

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
            version: "3.4.45".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "新增 [prefix-drift] / [prefix-shrink] 诊断日志：对比同会话相邻两次请求的 tools 与 history[1..] 指纹，定位前缀从第几条消息起发生变化，用于区分前缀改动与上游缓存丢失",
                    "Added [prefix-drift] / [prefix-shrink] diagnostics: tools and history[1..] fingerprints are compared between consecutive requests in a session to pinpoint the first message where the prefix changed, telling prefix edits apart from upstream cache loss",
                )]),
                improve_group(vec![
                    Bilingual::new(
                        "36 套主题的文字与背景对比度统一提升到 WCAG AA（4.5:1），主题设置页实时预览的文字按实际底色计算可读色",
                        "Text-to-background contrast across all 36 themes now meets WCAG AA (4.5:1), and the live preview on the theme settings page picks a readable text color from the actual background",
                    ),
                    Bilingual::new(
                        "普通提示 toast 显示时长缩短到 2 秒；错误提示延长到 5 秒，带操作按钮的提示为 6 秒，留出阅读和点击时间",
                        "Regular toasts now dismiss after 2 seconds; error toasts stay for 5 seconds and toasts with an action button for 6 seconds, leaving time to read and click",
                    ),
                    Bilingual::new(
                        "从侧栏其他页面切回账号管理页时，刷新内容与浏览器标签页切回前台保持一致",
                        "Switching back to the account manager from another sidebar page now refreshes the same data as returning to the browser tab",
                    ),
                    Bilingual::new(
                        "支持模型页改为固定视口高度，模型表在框内滚动；按模型分组的折叠标题仅箭头和文字可点击",
                        "The supported-models page now has a fixed viewport height with the table scrolling inside its frame; per-model group headers toggle only from the arrow and label",
                    ),
                ]),
                fix_group(vec![
                    Bilingual::new(
                        "缓存读取反推改用全部 Claude 模型的受控实测费率（未缓存 / 缓存 / 输出三项），修复 Sonnet 5.5 等模型缓存读取几乎恒为 0；未实测模型跳过反推、保留前缀估算",
                        "Cache-read inference now uses measured rates (uncached / cached / output) for every Claude model, fixing cache reads that were almost always 0 on Sonnet 5.5 and others; unmeasured models skip inference and keep the prefix estimate",
                    ),
                    Bilingual::new(
                        "基线表淘汰改为优先保留被反复匹配的主对话基线，避免子代理连续登记新基线把主对话基线挤出，导致上游前缀缓存整段失效",
                        "Baseline eviction now favours main-conversation baselines that keep getting matched, so bursts of sub-agent baselines no longer push them out and invalidate the whole upstream prefix cache",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.43".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "聊天回复末尾新增 turn/session credits 展示（showSessionCredits）：对话结束时把本次所耗与累计的 credits 一并呈现给用户",
                    "The end of a chat reply now shows turn/session credits (showSessionCredits): on conversation end the credits consumed this turn and the running session total are presented to the user",
                ),
                Bilingual::new(
                    "管理端弹窗与下拉菜单增加进出场动画，交互过渡更平滑",
                    "The admin-side dialogs and dropdown menus now animate on open and close, making transitions smoother",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.42".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "账号管理与使用日志页切回浏览器标签页时自动刷新数据，避免长时间停留在页面后展示过期状态",
                "The account-manager and usage-log pages now auto-refresh their data when the browser tab regains focus, so stale state is not shown after the page has been idle",
            )])],
        },
        ReleaseNote {
            version: "3.4.41".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "设置页「Kiro 请求体落盘取证」新增网页端浏览与下载：管理员可在弹窗中列出落盘文件、逐个下载或一键打包 ZIP 下载，容器化部署无需再通过 SSH / docker cp 取回；下载走 x-api-key 头鉴权，凭据不进查询参数。同时收紧了落盘文件的清理判据（仅识别规范命名的 dump 文件，手工放入的其他 json 不再被计数或删除）并为写入加锁，修复并发写入可能超出 200 个上限的竞态",
                    "The Kiro request-body dump setting now offers in-browser browsing and download: admins can list dump files in a dialog, download them one by one or as a single ZIP, with no need for SSH / docker cp in containerised deployments. Downloads authenticate with the x-api-key header, so credentials never appear in query strings. The cleanup predicate is also tightened (only well-formed dump file names are recognised; hand-placed json files are no longer counted or deleted) and writes are serialised, fixing a race where concurrent writes could exceed the 200-file cap",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复编辑账号时 Client Secret 被浏览器自动填充的管理密码覆盖（#58）：登录页保存的管理密码会被填进弹窗里的 password 框，保存后被当作 clientSecret 写入，要等 token 到期刷新才报 invalid_client 并被禁用，且管理密码会随刷新请求发往 AWS。现在 password 输入框默认禁用浏览器与 1Password / LastPass / Bitwarden 的自动填充；后端在 clientId 或 IdC 账号的 clientSecret 改为非空新值时，先用更新后的配置（含同次提交的代理与区域）试刷新，失败则拒绝保存；更新日志记录涉及的字段名（不含值）。已被覆盖的账号需重新填入真实的 clientSecret",
                    "Fixed the Client Secret being overwritten by a browser-autofilled admin password when editing an account (#58): the admin password saved from the login page was autofilled into password fields in the dialog and saved as clientSecret, only surfacing as invalid_client at the next token refresh (after which the account was disabled), and the admin password would be sent to AWS with each refresh. Password inputs now opt out of browser and 1Password / LastPass / Bitwarden autofill by default; when clientId, or clientSecret on an IdC account, is changed to a non-empty value the backend first trial-refreshes with the updated configuration (including a proxy or region changed in the same request) and rejects the save on failure; the update log now records the names of the fields involved (never values). Accounts already overwritten need their real clientSecret entered again",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.40".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "输出 Tokens 口径由字符估算改为 tiktoken BPE 真值，与思考徽章同口径：修复「总输出 Token 数少于深度思考 Token 数」的矛盾（旧口径对思考内容按不同比例估算，导致两者难以直接比较）。流式与非流式桥接路径均已切换，工具调用参数一并计入",
                        "Output token counting switched from character estimation to real tiktoken BPE values, sharing the same tokenizer as the thinking badge. This fixes the contradiction where total output tokens appeared smaller than the thinking token count (the old formula estimated thinking content with a different ratio, making the two numbers incomparable). Both streaming and non-streaming bridge paths are migrated, and tool-call arguments are included",
                    ),
                    Bilingual::new(
                        "根据上游 credits 消耗校准缓存读取用量：结合静态前缀上界与 creditsUsed 反推实际命中的缓存 token 数，接入流式落库与非流式响应（此前 Kiro 的 meteringEvent 不返回 cache_read 真值，缓存读取长期不可观测）",
                        "Cache-read usage is now calibrated from upstream credit consumption: the actual cached token count is inferred by combining the static prefix upper bound with creditsUsed, wired into streaming persistence and non-streaming responses (Kiro's meteringEvent does not report cache_read, so cache reads were previously unobservable)",
                    ),
                    Bilingual::new(
                        "新增 [cache-miss] INFO 诊断日志：credits 反推发现上游缓存部分未命中（命中率 < 80%）时输出命中率、缺失 token 数、与上次结算的闲置间隔及凭据是否变化，用于区分 TTL 逐出、换账号与上游换实例三种原因",
                        "Added [cache-miss] INFO diagnostics: when credit inference shows a partial upstream cache miss (hit rate < 80%), the log reports the hit ratio, missing tokens, the idle gap since the previous settlement and whether the credential changed, helping tell apart TTL eviction, account switching and upstream instance changes",
                    ),
                ]),
                improve_group(vec![
                    Bilingual::new(
                        "缓存漂移诊断提升为 INFO 并按请求类型匹配快照：同一会话混入的标题生成、额度探测、子代理等辅助请求不再被误报为主对话漂移；每个会话保留至多 4 份快照，按系统块重叠字节数匹配最相似的一份，与所有快照都无重叠时标注为「新请求类型」",
                        "Cache drift diagnostics moved to INFO and now match snapshots by request type: auxiliary requests sharing a session (title generation, quota probes, sub-agents) are no longer misreported as main-conversation drift. Up to four snapshots are kept per session and matched by shared system-block bytes; a request overlapping none of them is labelled a new request type",
                    ),
                    Bilingual::new(
                        "首条消息、单条消息重发及 history[0] 重置轮次不再把尚未建立缓存的 system + tools 误计入缓存读取，报告的 cache_read 更贴近实际分母",
                        "The first message of a window, single-message resends and turns that reset history[0] no longer count the not-yet-cached system + tools as cache reads, so reported cache_read better matches the real denominator",
                    ),
                    Bilingual::new(
                        "使用日志新增缓存命中率胶囊，深度思考 Token 数改为琥珀色胶囊样式",
                        "The usage log now shows a cache hit-rate pill, and the thinking token count uses an amber pill style",
                    ),
                ]),
                fix_group(vec![Bilingual::new(
                    "暂隐藏使用日志的缓存命中百分比胶囊：meteringEvent 不返回 cache_read 真值，命中率完全依赖 credits 反推，而反推在 credits 高于全未命中基线时被截断为 0%，会把缓存写入轮误显成 0%。口径校准前先隐藏，功能开关保留在代码中可随时恢复；缓存读取数字行不受影响",
                    "Temporarily hide the cache hit-rate pill in the usage log: meteringEvent does not report cache_read, so the rate relies entirely on credit inference, which clamps to 0% whenever credits exceed the all-miss baseline and misrenders cache-write turns as 0%. Hidden until the metric is calibrated, with the switch kept in code for easy restore; the cache-read token row is unaffected",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.39".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "终端入口（cli）的思考文本由变暗改为变暗 + 斜体，与其他入口的引用块 + 斜体样式视觉一致；历史里旧版本渲染的变暗文本回传上游时仍会被整块剥离",
                        "Thinking text in the terminal entry (cli) is now rendered dim + italic instead of dim only, matching the blockquote + italic style used by other entries; legacy dim-only text in client history is still stripped before being sent upstream",
                    ),
                    Bilingual::new(
                        "Admin 模型页在模型 ID 下方展示上游返回的模型描述（悬停显示全文）；上游未返回时不渲染",
                        "The Admin models page now shows the upstream model description under the model ID (full text on hover); nothing is rendered when upstream does not return one",
                    ),
                ]),
                fix_group(vec![Bilingual::new(
                    "修复 Claude Code 长会话中 credits 偏高的一个原因：CC 中途追加的“任务工具提醒”（The task tools haven't been used recently…）被并入 history[0]，每触发一次就使上游前缀缓存整段失效。现将其归入动态块，改放到当前消息开头，模型仍可见。新增 DEBUG 级漂移诊断日志（需开启 debugLogs）：缓存 key 变化时列出新增的系统块，便于定位其他类似提醒",
                    "Fix one cause of inflated credits in long Claude Code sessions: the mid-session \"task tools haven't been used recently\" reminder was merged into history[0], invalidating the whole upstream prefix cache each time it fired. It is now treated as a dynamic block and moved to the start of the current message, so the model still sees it. Adds DEBUG-level drift diagnostics (requires debugLogs) that list newly added system blocks whenever the cache key changes, to help locate similar reminders",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.38".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Codex Desktop 通过 /v1/responses 接入时所有 MCP 工具和子代理工具都不可用的问题：此前只展开 functions 命名空间，其余 namespace 容器整体被丢弃；现在其子工具展开为 ns__叶子名 发给上游，并在响应里还原为 function_call 的 namespace + 叶子 name，历史回放同步使用同一命名规则；namespace 内的 custom 工具仍会被跳过。注意：被发送的工具数量增加，输入 token 会相应上升（#57）",
                "Fix all MCP and sub-agent tools being unavailable when Codex Desktop connects via /v1/responses: only the functions namespace used to be expanded and every other namespace container was dropped. Their child tools are now flattened to ns__leaf for the upstream, restored to function_call namespace + leaf name in responses, and replayed history uses the same naming rule; custom tools inside a namespace are still skipped. Note: more tools are now sent, so input tokens increase accordingly (#57)",
            )])],
        },
        ReleaseNote {
            version: "3.4.37".to_string(),
            is_latest: false,
            groups: vec![
                improve_group(vec![Bilingual::new(
                    "思考文本化在非 cli 入口（Claude Desktop、local-agent 等）改用 Markdown 引用块 + 斜体来近似变暗，不含任何转义序列，避免出现 ESC 方块与字面 [2m/[0m（#54）；cli 入口仍使用 ANSI 变暗样式",
                    "Thinking-as-text on non-cli entries (Claude Desktop, local-agent, etc.) now renders as a Markdown blockquote with italics to approximate dimming, with no escape sequences, avoiding ESC blocks and literal [2m/[0m (#54); the cli entry still uses ANSI dimming",
                )]),
                fix_group(vec![
                    Bilingual::new(
                        "修复 Claude Code 开启 advisor 后整个会话每次请求都返回 400 的问题：内联 system 消息中的 tool_addition / tool_removal 事件现在会被跳过，同条消息里的文本照常保留；type 为 advisor_* 的服务端工具不再作为普通工具转发给 Kiro（Kiro 无对应能力，本次仅做兼容降级，并不提供 advisor 功能本身）（#56）",
                        "Fix HTTP 400 on every request once Claude Code enables advisor: tool_addition / tool_removal events in inline system messages are now skipped while text in the same message is kept, and server tools whose type starts with advisor_ are no longer forwarded to Kiro as ordinary tools (Kiro has no equivalent; this is a compatibility fallback only and does not provide advisor itself) (#56)",
                    ),
                    Bilingual::new(
                        "修复历史回传时剥离思考文本过度的问题：只剥离渲染器补充的换行，降级路径（流被中断、缺少时长行）仅拆开头的引用段，避免误删正文",
                        "Fix over-aggressive stripping of rendered thinking text in replayed history: only newlines added by the renderer are removed, and the fallback path (interrupted stream without a duration line) now unwraps only the leading quote run, so answer text is no longer clipped",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.36".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![
                Bilingual::new(
                    "修复 Claude Desktop Cowork 模式下思考内容出现 ESC 方块与字面 ANSI 转义序列（如 [2m/[0m）的格式异常：思考文本化输出改为仅 cli 入口（或解析不出入口的老格式 UA）使用 ANSI 变暗样式，Claude Desktop / local-agent / VS Code / SDK 等其余入口一律用纯文本，任何客户端下都不会乱码（#54）",
                    "Fix garbled thinking content in Claude Desktop Cowork mode (ESC blocks and literal ANSI sequences like [2m/[0m): thinking-as-text output now applies ANSI dimming only for the cli entry (or legacy UAs without a parseable entry); all other entries — Claude Desktop / local-agent / VS Code / SDK — fall back to plain text, which renders correctly in every client (#54)",
                ),
                Bilingual::new(
                    "修复管理面板账号详情、API Key 详情与失败/限流日志页滚动到底出现大块空白的问题：根容器改为确定高度 flex 布局，消除页面级与表格内层的双层滚动，页面级滚动归零后仅保留表格单层滚动",
                    "Fix a large blank area when scrolling to the bottom of the account detail, API Key detail and failure/throttle log pages: the root container now uses a fixed-height flex layout, eliminating the dual page-level + table-level scrollbars so only the table scrolls internally",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.35".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "账号余额弹窗新增多时间窗使用统计：累计 / 今日 / 7 天 / 30 天四组统计卡片，各自展示请求数、输入输出 Tokens、花费与 Credits 消费；新增 /credentials/{id}/usage/today|7days|30days 端点，按 CST（UTC+8）自然日聚合，口径与累计统计一致（受明细缓冲裁剪约束）",
                    "Multi-range usage stats in the account balance dialog: four stat card groups — total / today / 7 days / 30 days — each showing requests, input/output tokens, cost and credits consumption; backed by new /credentials/{id}/usage/today|7days|30days endpoints aggregated by CST (UTC+8) calendar day, same scope as the total stats (bounded by the retained-records cap)",
                )]),
                fix_group(vec![Bilingual::new(
                    "i18n 英文文案润色：修正 20 余处不地道或歧义表述，包括批量操作文案缺失批量语义（Restore Failed Accounts、Batch Disable 等）、中式英语（Re-query → Refresh balance、Release to import → Drop the file to import、Real Credits → Actual Credits 等）与冠词、大小写细节",
                    "i18n English copy polish: fixed 20+ non-idiomatic or ambiguous phrasings, including batch-action labels missing the batch semantic (Restore Failed Accounts, Batch Disable, etc.), unnatural wording (Re-query → Refresh balance, Release to import → Drop the file to import, Real Credits → Actual Credits, etc.) and article/capitalization details",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.34".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "账号编辑弹窗重构：已有信息（邮箱、登录来源、Auth Region、Client ID、Profile ARN、Machine ID）改为「有值即锁定」只读展示，无值字段仍可补录；表单按「核心凭据 / 身份与区域 / 运行与网络」三分区重排，分区卡片、分段器与头部图标统一为 credential-form-kit 共享视觉基元；账号状态接口随之下发 clientId / profileArn / machineId（均非机密字段）",
                        "Account edit dialog redesign: existing values (email, login source, Auth Region, Client ID, Profile ARN, Machine ID) are shown read-only under a value-locks-the-field rule, while empty fields stay editable for backfill. The form is regrouped into Core Credentials / Identity & Region / Runtime & Network sections, with section cards, segmented controls and the header icon unified into a shared credential-form-kit; the credential status API now returns clientId / profileArn / machineId (all non-secret)",
                    ),
                    Bilingual::new(
                        "使用日志页与账号详情页新增总费用 / 总 Credits / 总 Token 统计：使用日志页指标条扩为 7 项（区间总计 + 当页汇总）；账号详情页新增累计统计卡，配套新端点 /credentials/{id}/usage/summary（按账号聚合全部留存记录，口径与该页总请求数一致，不含已裁剪历史）；日报 DailySummary 补充 totalOutputTokens",
                        "Total cost / total credits / total tokens stats on the usage log page and account detail page: the usage log metrics bar expands to 7 items (range totals plus current-page rollups); the account detail page gains cumulative stat cards backed by a new /credentials/{id}/usage/summary endpoint that aggregates all retained records for an account (same scope as its total-request count, trimmed history excluded); DailySummary now includes totalOutputTokens",
                    ),
                ]),
                improve_group(vec![Bilingual::new(
                    "账号弹窗细节打磨：分区卡片与页脚按钮右缘精确对齐关闭按钮（19px 同线）；弹窗滚动条整体隐藏（滚动仍可用，各平台口径一致）；只读字段与双输入组补齐 role / aria-label 无障碍语义",
                    "Account dialog polish: section cards and footer buttons align flush with the close button's right edge (19px line); dialog scrollbars are hidden while scrolling still works, consistent across platforms; read-only fields and paired inputs gain role / aria-label accessibility semantics",
                )]),
                fix_group(vec![Bilingual::new(
                    "修复编辑弹窗在列表自动刷新场景下的健壮性问题：初始值快照补 priority 参与差异比对，避免弹窗打开期间他人修改优先级后被旧值覆盖；快照守卫重构为可空 state + useLayoutEffect，修复关闭态对 null 取属性导致整页白屏；刷新按钮加防重入锁并统一禁用/旋转加载态；Credits 大数改紧凑缩写防窄格溢出；统计单位文案接入 i18n",
                    "Robustness fixes for the edit dialog under automatic list refresh: the initial-value snapshot now includes priority in diff detection so a priority changed by someone else mid-edit is no longer silently overwritten with a stale value; the snapshot guard is rebuilt as nullable state plus useLayoutEffect, fixing a whole-page white screen caused by dereferencing null while the dialog is closed; the refresh button gains a re-entry lock with unified disabled/spinning states; large credit numbers switch to compact notation to avoid narrow-cell overflow; stat unit labels are moved into i18n",
                )]),
            ],
        },
        ReleaseNote {
            version: "3.4.33".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "admin-ui 新增「使用文档」页：按设计稿重写为 sticky hero + 计数角标分区 tabs + STEP 01/02/03 三卡流转 + Mac 窗体代码工作台，内置快速接入、开发指南、接口参考、排障指南四大分区，支持 hash 深链、TOC 滚动跟随、API Key 自动填充与 CC Switch 一键导入；账号管理页与 API Keys 页的 GitHub README 外链改为站内文档入口",
                    "New Usage Docs page in admin-ui: rewritten to match the design with a sticky hero, count-badged section tabs, a STEP 01/02/03 flow, and a Mac-style code workbench. It ships Quick Start, Development, API Reference and Troubleshooting sections with hash deep links, scroll-following TOC, API key auto-fill and one-click CC Switch import; the GitHub README links on the Accounts and API Keys pages now point to this in-app page",
                )]),
                fix_group(vec![
                    Bilingual::new(
                        "文档页 Codex CLI 配置补齐 env_key = \"KIRO_API_KEY\" 且 .env 变量名同步，此前照抄页面配置首次请求会因不带密钥返回 401；接口参考的示例命令改用已选中的实际 API Key，不再固定 <API_KEY> 占位符；未选择 Key 时 CC Switch 卡片改为纯提示，不再把提示文案当链接复制",
                        "Docs page Codex CLI config now includes env_key = \"KIRO_API_KEY\" with a matching .env variable name — copying the previous snippet returned 401 on the first request for lack of a key. API Reference examples use the selected key instead of a fixed <API_KEY> placeholder, and the CC Switch card shows plain text instead of making the hint copyable when no key is selected",
                    ),
                    Bilingual::new(
                        "文档页细节修正：hash 跳转后的 TOC 高亮锁定改用定时器状态，不再在渲染期读取时钟，并修复清空 fragment 后高亮永久失效；代码块复制定时器补清理，避免连续点击堆叠与卸载后 setState；客户端切换改用 aria-pressed 按钮修正无障碍语义；排障示例标题补英文翻译，端点数量描述与端口展示修正",
                        "Docs page fixes: the post-hash-jump TOC highlight lock now uses a timer-backed state instead of reading the clock during render, and no longer gets stuck after the fragment is cleared; the code block copy timer is cleaned up to avoid stacked timeouts and setState after unmount; client tabs use aria-pressed buttons with correct semantics; troubleshooting example labels are translated, and the endpoint count wording and port display are corrected",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.32".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![Bilingual::new(
                    "设置页新增 DEBUG 日志开关（debugLogs）：控制台、日志文件与「查看日志」页三处同步输出 DEBUG 级别事件，可热切换并写回 config.json，无需重启；RUST_LOG 指令仍优先，开关只升不降",
                    "New DEBUG logs switch (debugLogs) in Settings: console, log file and the Logs page output DEBUG events together; hot-toggled and persisted to config.json without a restart. RUST_LOG directives still take precedence and the switch only raises verbosity",
                )]),
                fix_group(vec![
                    Bilingual::new(
                        "修复 Claude Code Read 工具返回的图片被静默丢弃（issue #52，感谢 @jayus71 的 PR #53）：tool_result 内嵌图片现在随请求转发给 Kiro；并行读多张图时，图片说明改用整条消息内的全局序号 [Image n attached from tool_result <id>]，让模型能把图对应到工具结果（实测并行读三张图顺序正确率由约 1/6 提升到 5/6）；无法转发的图片（如 SVG）留下占位，不再让模型误以为工具返回了空内容；合并历史消息与过滤孤立 tool_result 后序号保持与最终图片位置一致",
                        "Fixed images returned by Claude Code's Read tool being silently dropped (issue #52, thanks to @jayus71 for PR #53): images embedded in tool_result are now forwarded to Kiro. When several images are read in parallel, captions use a message-wide ordinal [Image n attached from tool_result <id>] so the model can map each image to its tool result (parallel three-image order accuracy rose from about 1/6 to 5/6 in testing). Images that cannot be forwarded (e.g. SVG) leave a placeholder instead of looking like an empty tool result, and ordinals stay aligned with final image positions after merging history messages and dropping orphaned tool_results",
                    ),
                    Bilingual::new(
                        "修复 OpenAI Responses 端点 function_call_output / custom_tool_call_output 中的图片被丢弃（Codex view_image、截图类工具）：data URL 图片现在进入 Kiro images，远程 URL 与转换失败的图片降级为文本占位，不再产出空内容",
                        "Fixed images in OpenAI Responses function_call_output / custom_tool_call_output being dropped (Codex view_image and screenshot tools): data URL images now reach Kiro images, while remote URLs and images that fail to convert degrade to text placeholders instead of producing empty content",
                    ),
                    Bilingual::new(
                        "缓存断点记账修正：本地缓存写入与 metering 上报口径解耦，避免后续请求回落本地模拟时读不到前缀；defer_loading 工具移出顶层缓存前缀，改在 tool_reference 引用处展开定义，仅改被引用工具的 schema 时不再误报缓存命中；规范化键做 JSON 转义，避免键 a:1,b 与键 a、b 同哈希。仅在 promptCacheAccounting 开启时影响本地 usage 计量",
                        "Cache breakpoint accounting fixes: local cache commits are decoupled from metering reporting so later requests falling back to local simulation can still read the prefix; defer_loading tools leave the top-level cache prefix and are expanded at their tool_reference sites, so changing only a referenced tool's schema no longer reports a false cache hit; canonical keys are JSON-escaped so key a:1,b no longer hashes like keys a and b. Only affects local usage accounting when promptCacheAccounting is enabled",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.31".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "新增 promptCacheAccounting 开关：按 Anthropic cache_control 断点规则在本地模拟缓存记账，cache_read / cache_creation（含 5m/1h 拆分）按请求断点、20 block 回看与 TTL 计算，取代「除最后一条外全部计为缓存读」的前缀估算；缓存按 API Key 隔离并落盘 prompt_cache.json，重启后恢复，OpenAI 兼容端点按自动前缀缓存处理。配套 promptCacheMinTokens 覆盖最小可缓存长度，outputTokensIncludeThinking 让上报的 output_tokens 含 thinking（官方口径）。默认关闭，行为与此前一致（本地模拟，不代表 Kiro 真实成本）",
                        "New promptCacheAccounting switch: simulates prompt cache accounting locally following Anthropic cache_control breakpoint rules — cache_read / cache_creation (including the 5m/1h split) are computed from request breakpoints, the 20-block lookback and TTL, replacing the prefix estimate that counted everything except the last message as cache reads. Caches are isolated per API Key and persisted to prompt_cache.json, restored on restart; OpenAI-compatible endpoints use automatic prefix caching. Companion options: promptCacheMinTokens overrides the minimum cacheable length, and outputTokensIncludeThinking makes reported output_tokens include thinking (official semantics). Disabled by default with unchanged behavior (local simulation, not Kiro's real cost)",
                    ),
                    Bilingual::new(
                        "/v1/models 新增 modelsOfficialAliases 开关：为上游点号模型 ID 追加官方连字符别名（如 claude-opus-4.6 → claude-opus-4-6，4.5 代际额外追加带日期 ID），供按模型列表生成映射的下游网关使用；默认关闭，避免 Claude Code 模型选择器出现重复项",
                        "New modelsOfficialAliases switch for /v1/models: appends official hyphenated aliases to upstream dotted model IDs (e.g. claude-opus-4.6 → claude-opus-4-6, with dated IDs added for the 4.5 generation) for downstream gateways that build model mappings from the list; disabled by default to avoid duplicate entries in the Claude Code model picker",
                    ),
                ]),
                fix_group(vec![
                    Bilingual::new(
                        "修复 Suggestion Mode 拦截后的空响应写死 model=\"suggestion\"、output_tokens=1：改为回显请求 model 并上报零用量，避免下游按响应记账的网关误判模型不一致并产生计费",
                        "Fixed the intercepted Suggestion Mode response hardcoding model=\"suggestion\" and output_tokens=1: it now echoes the requested model and reports zero usage, so downstream gateways that bill from responses no longer flag a model mismatch or charge for it",
                    ),
                    Bilingual::new(
                        "断点记账按评审与 CR 意见收紧：缓存按 API Key + 映射后 Kiro 模型 ID 隔离，低于最小长度的断点不建条目；默认关闭路径恢复零拷贝，缓存表淘汰保留 10% 滞回，快照持锁仅复制条目、编码移到锁外，后台任务 JoinError 与快照读取失败补 warn 日志；API Key 详情页逐模型用量折叠标题恢复 h3 语义",
                        "Tightened breakpoint accounting per review and code-review feedback: caches are isolated by API Key plus mapped Kiro model ID, breakpoints below the minimum length create no entry; the default-off path is zero-copy again, eviction keeps a 10% hysteresis, snapshots copy entries under lock and encode outside it, and warn logs cover JoinError and snapshot read failures; the per-model usage collapsible title on the API Key detail page regains h3 semantics",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.30".to_string(),
            is_latest: false,
            groups: vec![
                feat_group(vec![
                    Bilingual::new(
                        "请求明细页全面接入「按模型分组（当前页）」可折叠统计面板：账号详情、API Key 详情、使用日志（今日/自定义区间）、日明细五处同款，聚合当前页请求的请求数/Token/费用/积分，折叠偏好跨页面切换保持",
                        "Per-model grouping (current page) collapsible panel rolled out across all request-detail views: credential detail, API Key detail, usage log (today/custom range) and daily detail share the same panel aggregating requests/tokens/cost/credits of the current page, with the collapse preference preserved across page switches",
                    ),
                    Bilingual::new(
                        "新增一键构建并启动脚本 build-and-run（mac/Windows）：先构建后启动，任一环节失败即中止，不会用旧二进制启动；Shell 快捷命令同步新增 build_and_run_kiro2cc_proxy",
                        "New build-and-run one-click scripts (mac/Windows): build then run, aborting on any failure so a stale binary is never started; the build_and_run_kiro2cc_proxy shell alias is added in sync",
                    ),
                ]),
                fix_group(vec![
                    Bilingual::new(
                        "修复 local-agent 等入口思考文本化静默失效（v3.4.23 引入的回归）：入口判定改为黑名单，只有确认渲染不了 ANSI 的 claude-desktop* 用纯文本样式，其余 Claude Code 入口一律 ANSI，恢复思考过程实时可见（issue #48）",
                        "Fixed silently disabled thinking-as-text for local-agent and other entrypoints (a regression introduced in v3.4.23): entrypoint routing switches to a blacklist — only claude-desktop*, which cannot render ANSI, uses plain text, while every other Claude Code entrypoint gets ANSI, restoring live thinking output (issue #48)",
                    ),
                    Bilingual::new(
                        "修复账号管理「刷新」按钮不更新今日消耗积分等每日统计：补上与切 tab 刷新同款的统计查询失效；本地启动脚本改为轮询服务就绪后再自动打开管理面板，消除页面加载失败需手动 reload 的竞态；Windows 构建脚本补齐退出码检查",
                        "Fixed the account Refresh button not updating today's credits and other daily stats by invalidating the stats query like tab-switch refresh does; local startup scripts now poll until the service is ready before auto-opening the admin panel, removing the load-failure race that required a manual reload; Windows build scripts now check exit codes",
                    ),
                ]),
            ],
        },
        ReleaseNote {
            version: "3.4.27".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "请求日志新增「用时」列：流式记录展示首字/耗时双徽章（按快慢分档着色），非流式仅耗时；首字埋点按客户端可见产出判定，桥接 web_search 计入、空事件不计",
                    "Usage log gains a Duration column: streaming records show first-token/duration badges tinted by speed, non-streaming shows duration only; first-token timing is keyed to client-visible output — bridged web_search counts, empty events don't",
                ),
                Bilingual::new(
                    "请求日志新增深度思考 token 统计：🧠 徽章内联在输出 Tokens 旁，BPE 真值口径，与客户端「Thought for Ns (N tokens)」显示一致",
                    "Usage log gains thinking-token statistics: a 🧠 badge inline next to output tokens, BPE-exact, consistent with the client's \"Thought for Ns (N tokens)\" display",
                ),
                Bilingual::new(
                    "支持上游新增的 kimi-k3 模型：关键词映射、静态模型表条目、支持模型页归入 KIMI 家族；深度思考按第三方模型惯例走文本标签协议",
                    "Supports the new upstream kimi-k3 model: keyword mapping, static model list entry, KIMI family grouping on the models page; deep thinking follows the third-party text-tag convention",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.26".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "admin-ui 新增主题设置页：37 套色彩主题（经典网站/浅色/深色/编辑器分组）× 明暗双模式自由组合，带实时预览色板与 View Transition 圆形扩散切换动画，选择持久化到 localStorage；侧栏顶栏新增主题入口，支持一键恢复默认外观",
                "admin-ui gains a Theme Settings page: 37 color themes (website/light/dark/editor groups) freely combined with light/dark modes, featuring live preview swatches and a View Transition circular reveal animation, with the selection persisted to localStorage; a theme entry is added to the sidebar and top bar, with one-click reset to the default appearance",
            )])],
        },
        ReleaseNote {
            version: "3.4.25".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "混合账号池按账号实际可用模型路由：调度前用各账号 ListAvailableModels 探测结果前置过滤，请求不再被分发到不支持目标模型的账号（此前上游直接拒绝且不触发故障转移）。探测结果缓存 30 分钟、失败 2 分钟后重试，稳态零网络开销；探测失败/空列表视为未知不过滤，候选全部明确不支持时回退为不过滤（issue #49）",
                "Model-aware routing for mixed account pools: account availability is probed via ListAvailableModels and requests are no longer dispatched to accounts that do not support the target model (previously rejected upstream without failover). Probe results are cached for 30 minutes with a 2-minute retry after failure, at zero steady-state network cost; failures/empty lists are treated as unknown (no filtering), and when all candidates are known-unsupported the filter is skipped (issue #49)",
            )])],
        },
        ReleaseNote {
            version: "3.4.24".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Claude Desktop 客户端开启 thinkingAsText 时思考内容出现 ANSI 转义乱码（[2m / [0m 与方块字符）：终端能渲染 ANSI，但 Desktop 是图形界面不解析，ESC 直接显示为乱码。现按 User-Agent 括号内的入口字段选择渲染样式——cli 或解析不出入口仍用 ANSI（行为不变），claude-desktop* 改用无转义的纯文本（标记行与正文、时长行间留空行，正文换行用 Markdown 硬换行以适配渲染），其余入口（claude-vscode、sdk-ts 等渲染能力未知）不启用该功能、回退原生 thinking（issue #48）",
                "Fixed ANSI escape garbage ([2m / [0m and box characters) in thinking output when thinkingAsText is enabled on Claude Desktop: terminals render ANSI but Desktop is a GUI that does not parse it, so the ESC byte shows as garbage. The render style is now chosen by the entrypoint field in the User-Agent — cli or unparseable entrypoints keep ANSI (unchanged), claude-desktop* switches to escape-free plain text (blank lines around the marker and duration lines, Markdown hard breaks for body newlines), and other entrypoints (claude-vscode, sdk-ts, etc., whose rendering capability is unknown) leave the feature off and fall back to native thinking (issue #48)",
            )])],
        },
        ReleaseNote {
            version: "3.4.22".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![
                Bilingual::new(
                    "企业版 IdC 账号（IAM Identity Center）支持：新增企业备用 profileArn 兜底，避免企业账号被补上 BuilderId 占位 ARN 导致上游拒绝；添加账号时先调用 ListAvailableProfiles 自动发现真实 ARN 并写回；额度查询失败时自动发现 ARN 重试，仍失败则按 KAM 做法不带 ARN 查询（issue #43）",
                    "Enterprise IAM Identity Center accounts are now supported: an enterprise fallback profileArn prevents enterprise accounts from being given the BuilderId placeholder ARN that upstream rejects; when adding an account the real ARN is discovered via ListAvailableProfiles and written back; failed usage queries now discover the ARN and retry, falling back to an ARN-less query as KAM does (issue #43)",
                ),
                Bilingual::new(
                    "额度查询换区域重试改为按 HTTP 状态码判定（新增 UsageLimitsHttpError 携带状态码，Display 文案保持原格式以兼容错误分类）",
                    "The cross-region retry for usage queries is now decided by HTTP status code (a new UsageLimitsHttpError carries the status while its Display text keeps the previous format for error classification)",
                ),
            ])],
        },
        ReleaseNote {
            version: "3.4.21".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "分块写入约束的行数上限改为可配置：默认上限由 150 行放宽到 300 行，避免把本可一次写完的内容拆成多轮（客户端每轮都携带 thinking adaptive，轮数越多思考开销越大）；新增 writeChunkingMaxLines 配置项与环境变量 WRITE_CHUNKING_MAX_LINES，设为 0 即完全关闭该约束；可在 Admin 设置页热修改，仅影响之后的新请求（issue #46）",
                "The line limit of the chunked-writing constraint is now configurable: the default is relaxed from 150 to 300 lines so content that could be written in one call is no longer split across multiple turns (the client carries thinking adaptive on every turn, so more turns means more thinking overhead); a new writeChunkingMaxLines option and WRITE_CHUNKING_MAX_LINES environment variable were added, where 0 disables the constraint entirely; it can be changed live on the Admin settings page and only affects subsequent requests (issue #46)",
            )])],
        },
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
