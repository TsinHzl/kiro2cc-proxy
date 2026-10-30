// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! history[1..] 与 tools 的前缀漂移诊断（INFO 日志，不影响请求内容）
//!
//! `drift_diag` 只观测 history[0]（稳定系统块）。上游前缀缓存按字节匹配整个请求前缀
//! （系统 + tools + 历史消息），所以 history[1..] 里任意一条旧消息、或 tools 定义在两次
//! 请求之间发生变化，同样会让后面整段前缀 miss，而 history[0] 的日志里看不出来。
//!
//! 本模块对最终发往上游的 tools 与每条 history 消息分别取指纹，和同一会话上一次的快照比较。
//! 正常情况下上一次的 history 一定是这一次 history 的前缀（上一次的当前消息此时才变成
//! history 的最后一条）。不满足时分两类：
//! - 前缀的第 k 条消息不再一致（`Rewritten`）：缓存自第 k 条起整段失效，输出
//!   `[prefix-drift]`，写明序号、角色、新旧序列化字节数；
//! - 本次 history 更短但仍是上一次的前缀（`Truncated`，如回退 / 压缩）：共享部分照常命中，
//!   不是缓存失效，输出 `[prefix-shrink]`，避免被当成漂移排查；
//! - tools 指纹变化另输出一行 `[prefix-drift] tools ...`。
//!
//! 消息内容可能含用户输入（密钥等），所以 INFO 只给序号、角色和字节数；开头 80 个字符的
//! 预览只在 DEBUG 启用时（Admin 设置页的 `debugLogs`）附带。
//!
//! 与 `drift_diag` 相同的原则：每个会话保留少量快照，按「history[0] 相同且公共前缀最长」匹配，
//! 避免辅助请求（标题生成等）覆盖主对话快照；history[0] 与所有快照都不同视为新请求类型，
//! 不在这里报告（history[0] 变化由 `drift_diag` 负责）。只存指纹，不存请求内容。
//! INFO 未启用时跳过。

use std::collections::HashMap;
use std::hash::Hasher;
use std::sync::{Mutex, OnceLock};

use crate::kiro::model::requests::conversation::Message;
use crate::kiro::model::requests::tool::Tool;

/// 最多记录的会话数；超出后整体清空（诊断数据，丢失无影响）
const MAX_SESSIONS: usize = 256;
/// 每个会话最多保留的快照数（主对话 + 若干类辅助 / 子代理请求）
const MAX_SNAPSHOTS_PER_SESSION: usize = 4;
/// 新消息预览长度（字符数，仅 DEBUG 启用时输出）
const PREVIEW_CHARS: usize = 80;

/// 单条内容的指纹：序列化后的哈希 + 字节长度
type Fp = (u64, usize);

struct Snapshot {
    tools: Fp,
    history: Vec<Fp>,
}

static SNAPSHOTS: OnceLock<Mutex<HashMap<String, Vec<Snapshot>>>> = OnceLock::new();

/// history 相对上一次快照的变化
#[derive(Debug, PartialEq, Eq)]
enum HistoryDiff {
    /// 第 `index` 条起与上一次不一致，缓存自此失效；`prev_bytes` 为上一次该条的序列化字节数
    Rewritten { index: usize, prev_bytes: usize },
    /// 本次比上一次短，但仍是上一次的前缀（回退 / 压缩）：共享部分照常命中
    Truncated,
}

/// 相对上一次请求的全部变化
struct Drift {
    history: Option<HistoryDiff>,
    prev_history_len: usize,
    tools_changed: bool,
}

/// 把 `Hasher` 包装成 `io::Write`，让 serde_json 直接流式写入，避免为指纹分配整份 JSON 字符串
struct HashWriter {
    hasher: std::collections::hash_map::DefaultHasher,
    len: usize,
}

impl std::io::Write for HashWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hasher.write(buf);
        self.len += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn fingerprint<T: serde::Serialize + ?Sized>(value: &T) -> Fp {
    let mut w = HashWriter {
        hasher: std::collections::hash_map::DefaultHasher::new(),
        len: 0,
    };
    // 这里的类型都是普通结构体，序列化实际不会失败；万一失败，得到的是已写入部分的指纹，
    // 结果只是诊断多报一次，不影响请求
    let _ = serde_json::to_writer(&mut w, value);
    (w.hasher.finish(), w.len)
}

fn role_and_content(msg: &Message) -> (&'static str, &str) {
    match msg {
        Message::User(m) => ("user", m.user_input_message.content.as_str()),
        Message::Assistant(m) => ("assistant", m.assistant_response_message.content.as_str()),
    }
}

/// 两份历史指纹的公共前缀长度
fn common_prefix(a: &[Fp], b: &[Fp]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// 由公共前缀长度判定 history 的变化类型；上一次是本次前缀时返回 `None`
fn diff_history(prev: &[Fp], cur: &[Fp], prefix: usize) -> Option<HistoryDiff> {
    if prefix >= prev.len() {
        return None;
    }
    if prefix == cur.len() {
        return Some(HistoryDiff::Truncated);
    }
    Some(HistoryDiff::Rewritten {
        index: prefix,
        prev_bytes: prev[prefix].1,
    })
}

/// 锁内匹配并替换快照，返回相对上一次的变化（纯状态变更，不输出日志）
fn update_snapshots(session_id: &str, tools: Fp, history: &[Fp]) -> Option<Drift> {
    let map = SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() >= MAX_SESSIONS && !map.contains_key(session_id) {
        map.clear();
    }
    let snaps = map.entry(session_id.to_string()).or_default();
    // 只和 history[0] 相同的快照比较，取公共前缀最长的一份
    let best = snaps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.history.first() == history.first())
        .map(|(i, s)| (i, common_prefix(&s.history, history)))
        .max_by_key(|&(_, n)| n);
    let snapshot = Snapshot {
        tools,
        history: history.to_vec(),
    };
    let Some((i, prefix)) = best else {
        if snaps.len() >= MAX_SNAPSHOTS_PER_SESSION {
            snaps.remove(0);
        }
        snaps.push(snapshot);
        return None;
    };
    let prev = std::mem::replace(&mut snaps[i], snapshot);
    let history_diff = diff_history(&prev.history, history, prefix);
    let tools_changed = prev.tools != tools;
    (history_diff.is_some() || tools_changed).then_some(Drift {
        history: history_diff,
        prev_history_len: prev.history.len(),
        tools_changed,
    })
}

/// 第 `index` 条消息被改写：INFO 给序号 / 角色 / 新旧字节数，预览仅 DEBUG 附带
fn log_rewritten(
    session_id: &str,
    index: usize,
    prev_bytes: usize,
    prev_history_len: usize,
    history: &[Message],
    cur_history: &[Fp],
) {
    let role_content = history.get(index).map(role_and_content);
    let preview: Option<String> = if tracing::enabled!(tracing::Level::DEBUG) {
        role_content.map(|(_, c)| c.trim_start().chars().take(PREVIEW_CHARS).collect())
    } else {
        None
    };
    tracing::info!(
        session = %session_id,
        first_diff_index = index,
        prev_history_len,
        cur_history_len = history.len(),
        role = role_content.map(|(r, _)| r),
        prev_serialized_bytes = prev_bytes,
        new_serialized_bytes = cur_history.get(index).map(|f| f.1),
        preview = preview.as_deref(),
        "[prefix-drift] 历史前缀相对上一次请求发生变化（上一次的 history 本应是这一次的前缀），\
         缓存自第一处差异的消息起失效"
    );
}

/// 依据变化结果输出诊断日志
fn log_drift(
    session_id: &str,
    tools: &[Tool],
    history: &[Message],
    cur_history: &[Fp],
    drift: &Drift,
) {
    if drift.tools_changed {
        tracing::info!(
            session = %session_id,
            tools = tools.len(),
            "[prefix-drift] tools 定义相对上一次请求发生变化"
        );
    }
    match drift.history {
        Some(HistoryDiff::Rewritten { index, prev_bytes }) => log_rewritten(
            session_id,
            index,
            prev_bytes,
            drift.prev_history_len,
            history,
            cur_history,
        ),
        Some(HistoryDiff::Truncated) => tracing::info!(
            session = %session_id,
            prev_history_len = drift.prev_history_len,
            cur_history_len = history.len(),
            "[prefix-shrink] 本次 history 比上一次短（回退 / 压缩），共享前缀仍可命中，不是缓存失效"
        ),
        None => {}
    }
}

/// 记录本次请求最终发往上游的 tools 与 history，并在前缀相对上一次发生变化时输出诊断。
pub(super) fn observe(session_id: &str, tools: &[Tool], history: &[Message]) {
    if !tracing::enabled!(tracing::Level::INFO) {
        return;
    }
    let cur_tools = fingerprint(tools);
    let cur_history: Vec<Fp> = history.iter().map(fingerprint).collect();
    if let Some(drift) = update_snapshots(session_id, cur_tools, &cur_history) {
        log_drift(session_id, tools, history, &cur_history, &drift);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiro::model::requests::tool::{InputSchema, ToolSpecification};

    #[derive(Clone, Default)]
    struct BufWriter(std::sync::Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture<F: FnOnce()>(f: F) -> String {
        capture_at(tracing::Level::INFO, f)
    }

    fn capture_at<F: FnOnce()>(level: tracing::Level, f: F) -> String {
        let buf = BufWriter::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(level)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        String::from_utf8(buf.0.lock().unwrap().clone()).unwrap()
    }

    fn tool(name: &str) -> Tool {
        Tool {
            tool_specification: ToolSpecification {
                name: name.to_string(),
                description: format!("{name} tool"),
                input_schema: InputSchema::default(),
            },
        }
    }

    fn hist(contents: &[&str]) -> Vec<Message> {
        contents
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    Message::user(*c, "m")
                } else {
                    Message::assistant(*c)
                }
            })
            .collect()
    }

    #[test]
    fn appended_history_is_not_drift() {
        let tools = [tool("Read")];
        let out = capture(|| {
            let sid = "prefix-append";
            observe(sid, &tools, &hist(&["sys", "ack"]));
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1"]));
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1", "q2", "a2"]));
        });
        assert!(!out.contains("[prefix-drift]"), "{out}");
    }

    #[test]
    fn rewritten_old_message_reports_index_role_and_both_byte_sizes() {
        let tools = [tool("Read")];
        let out = capture(|| {
            let sid = "prefix-rewrite";
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1"]));
            // 第 3 条（a1）在下一次请求里被改写
            observe(
                sid,
                &tools,
                &hist(&["sys", "ack", "q1", "a1-EDITED", "q2", "a2"]),
            );
        });
        assert_eq!(out.matches("[prefix-drift]").count(), 1, "{out}");
        assert!(out.contains("first_diff_index=3"), "{out}");
        assert!(out.contains("role=\"assistant\""), "{out}");
        assert!(out.contains("prev_serialized_bytes="), "{out}");
        assert!(out.contains("new_serialized_bytes="), "{out}");
        assert!(!out.contains("[prefix-shrink]"), "{out}");
    }

    #[test]
    fn info_level_does_not_leak_message_content() {
        let tools = [tool("Read")];
        let out = capture(|| {
            let sid = "prefix-privacy-info";
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1"]));
            observe(
                sid,
                &tools,
                &hist(&["sys", "ack", "SECRET-TOKEN-abc123", "a1"]),
            );
        });
        assert_eq!(out.matches("[prefix-drift]").count(), 1, "{out}");
        assert!(!out.contains("SECRET-TOKEN"), "{out}");
    }

    #[test]
    fn debug_level_adds_message_preview() {
        let tools = [tool("Read")];
        let out = capture_at(tracing::Level::DEBUG, || {
            let sid = "prefix-privacy-debug";
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1"]));
            observe(
                sid,
                &tools,
                &hist(&["sys", "ack", "PREVIEW-VISIBLE-AT-DEBUG", "a1"]),
            );
        });
        assert_eq!(out.matches("[prefix-drift]").count(), 1, "{out}");
        assert!(out.contains("PREVIEW-VISIBLE-AT-DEBUG"), "{out}");
    }

    #[test]
    fn changed_tools_alone_is_reported() {
        let out = capture(|| {
            let sid = "prefix-tools";
            let h = hist(&["sys", "ack", "q1", "a1"]);
            observe(sid, &[tool("Read")], &h);
            observe(sid, &[tool("Read"), tool("Bash")], &h);
        });
        assert_eq!(out.matches("[prefix-drift]").count(), 1, "{out}");
        assert!(out.contains("tools 定义"), "{out}");
        assert!(!out.contains("first_diff_index"), "{out}");
    }

    #[test]
    fn tools_and_history_changes_are_reported_separately() {
        let out = capture(|| {
            let sid = "prefix-both";
            observe(sid, &[tool("Read")], &hist(&["sys", "ack", "q1", "a1"]));
            observe(
                sid,
                &[tool("Read"), tool("Bash")],
                &hist(&["sys", "ack", "q1-EDITED", "a1", "q2", "a2"]),
            );
        });
        assert_eq!(out.matches("[prefix-drift]").count(), 2, "{out}");
        assert!(out.contains("tools 定义"), "{out}");
        assert!(out.contains("first_diff_index=2"), "{out}");
    }

    #[test]
    fn shrunk_history_is_not_reported_as_drift() {
        let tools = [tool("Read")];
        let out = capture(|| {
            let sid = "prefix-shrink";
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1", "q2", "a2"]));
            // 回退：本次是上一次的严格前缀，共享部分仍可命中缓存
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1"]));
        });
        assert_eq!(out.matches("[prefix-shrink]").count(), 1, "{out}");
        assert!(!out.contains("[prefix-drift]"), "{out}");
        assert!(!out.contains("first_diff_index"), "{out}");
    }

    #[test]
    fn shrunk_then_rewritten_history_is_still_drift() {
        let tools = [tool("Read")];
        let out = capture(|| {
            let sid = "prefix-shrink-rewrite";
            observe(sid, &tools, &hist(&["sys", "ack", "q1", "a1", "q2", "a2"]));
            // 变短的同时，保留部分里的第 2 条也被改写：不是纯回退
            observe(sid, &tools, &hist(&["sys", "ack", "q1-EDITED", "a1"]));
        });
        assert_eq!(out.matches("[prefix-drift]").count(), 1, "{out}");
        assert!(out.contains("first_diff_index=2"), "{out}");
        assert!(!out.contains("[prefix-shrink]"), "{out}");
    }

    #[test]
    fn auxiliary_request_does_not_clobber_main_snapshot() {
        let tools = [tool("Read")];
        let out = capture(|| {
            let sid = "prefix-aux";
            observe(sid, &tools, &hist(&["main-sys", "ack", "q1", "a1"]));
            // 辅助请求：history[0] 不同，视为新请求类型，不报告
            observe(sid, &tools, &hist(&["title-sys", "ack"]));
            // 主对话正常追加：仍与主快照比较，无漂移
            observe(
                sid,
                &tools,
                &hist(&["main-sys", "ack", "q1", "a1", "q2", "a2"]),
            );
        });
        assert!(!out.contains("[prefix-drift]"), "{out}");
    }

    #[test]
    fn snapshots_per_session_are_capped() {
        let tools = [tool("Read")];
        let sid = "prefix-cap";
        capture(|| {
            for i in 0..(MAX_SNAPSHOTS_PER_SESSION + 3) {
                observe(sid, &tools, &hist(&[&format!("kind-{i}"), "ack"]));
            }
        });
        let map = SNAPSHOTS.get().unwrap().lock().unwrap();
        assert_eq!(map.get(sid).unwrap().len(), MAX_SNAPSHOTS_PER_SESSION);
    }
}
