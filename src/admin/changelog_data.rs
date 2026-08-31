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
    pub date: String,
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
            version: "3.0.17".to_string(),
            date: "2026-09-01".to_string(),
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
            date: "2026-08-21".to_string(),
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
            date: "2026-08-20".to_string(),
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
            date: "2026-08-19".to_string(),
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
            date: "2026-08-13".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "Admin 后台侧边栏支持折叠/展开",
                "Admin console sidebar now supports collapse/expand",
            )])],
        },
        ReleaseNote {
            version: "2.9.0".to_string(),
            date: "2026-08-07".to_string(),
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
            date: "2026-08-07".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "支持模型列表按模型家族分组排列",
                "Supported models list is now grouped by model family",
            )])],
        },
        ReleaseNote {
            version: "2.8.24".to_string(),
            date: "2026-08-06".to_string(),
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
            date: "2026-08-06".to_string(),
            is_latest: false,
            groups: vec![improve_group(vec![Bilingual::new(
                "移除主 API Key 全局兜底认证机制，收窄鉴权入口",
                "Removed the global fallback authentication via the master API key to narrow the authentication surface",
            )])],
        },
        ReleaseNote {
            version: "2.8.22".to_string(),
            date: "2026-08-05".to_string(),
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
            date: "2026-08-05".to_string(),
            is_latest: false,
            groups: vec![feat_group(vec![Bilingual::new(
                "每日统计页新增最近 14 天 credits 使用趋势曲线图",
                "Added a 14-day credits usage trend chart to the daily stats page",
            )])],
        },
        ReleaseNote {
            version: "2.8.20".to_string(),
            date: "2026-08-05".to_string(),
            is_latest: false,
            groups: vec![fix_group(vec![Bilingual::new(
                "修复 Dockerfile 缺少 COPY assets 导致容器内 ip2region xdb 缺失、构建失败的问题",
                "Fixed a build failure caused by the Dockerfile missing a COPY assets step, which left the ip2region xdb file absent in the container",
            )])],
        },
    ];

    // is_latest 后处理：与 CURRENT_VERSION 匹配的条目标为最新，
    // 列表里找不到时全部置 false（避免历史版本继续被高亮）
    let matched = notes.iter().filter(|n| n.version == CURRENT_VERSION).count();
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
                notes.iter().any(|n| n.version == CURRENT_VERSION && n.is_latest),
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
