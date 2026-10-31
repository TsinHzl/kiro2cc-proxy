// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 会话系统块基线：把会话中途新增的系统块分流出 history[0]
//!
//! history[0] 的缓存 key 是整段稳定系统内容的哈希。Claude Code 会在会话中途向 system 区追加新块
//! （按路径加载的 `~/.claude/rules/*.md`、各类状态通知……），每追加一次，key 变化，上游前缀缓存
//! 整段失效，下一轮要把整个上下文按未缓存重新计费（实测一次重置多花约 0.5 credits）。
//! `prompt::is_dynamic_hook_injection` 靠文本前缀识别已知的动态块，追不上 CC 的新花样；
//! 这里改用「是不是会话开始时就有的块」来判断，与块的文本无关。
//!
//! 做法：按会话记录基线（首次见到的稳定块指纹多重集）。之后的请求只要**包含全部基线块**，
//! 多出来的块就视为会话中途新增，不进 history[0]，由调用方放到当前消息开头——模型仍可见，
//! 但不参与缓存哈希。
//!
//! 不满足「包含全部基线块」的请求（子代理、压缩后的请求、已有块被原地改写）不被分流，
//! 并登记为该会话的新基线，行为与引入本模块之前一致。同一会话保留少量基线，
//! 避免子代理等辅助请求占掉主请求的基线；基线表满时优先淘汰过期的、被匹配次数少的
//! （见 `pick_victim`），所以子代理短时间连续登记新基线也挤不掉被反复使用的主对话基线。
//!
//! 误判防护：多出来的块总字节数必须不超过所匹配的基线本身的字节数。否则会出现
//! 「先到的是系统提示很短的辅助请求，随后到的主请求恰好包含它」，把主请求整份系统提示
//! 当作新增块搬走的情况。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::cache::{CacheEntry, evict_oldest_if_full};
use super::drift_diag::{Fingerprint, fingerprint};
use super::prompt::normalize_billing_header;

/// 单个会话最多保留的基线数（主请求 + 若干种辅助 / 子代理请求）
const MAX_BASELINES_PER_SESSION: usize = 4;

/// 基线空闲超过这个时长就视为过期，淘汰时优先清掉。
/// 空闲这么久，上游前缀缓存早已过期，丢掉基线不会多花钱。
const STALE_AFTER: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
struct Baseline {
    fps: Vec<Fingerprint>,
    /// 基线各块字节数之和，用于误判防护
    bytes: usize,
    last_used: Instant,
    /// 被后续请求匹配到的次数。主对话会被反复匹配，子代理 / 辅助请求的基线通常只有寥寥几次
    hits: u32,
}

/// 基线表已满时挑要淘汰的那个。
///
/// 只按「最久未使用」淘汰会出问题：同一个会话里子代理在几十秒内连续登记新基线，
/// 会把空闲了几分钟的主对话基线挤出去，主对话回来时被当成新请求，
/// 之前分流出去的块整批并回 history[0]，上游缓存整段失效（实测一次多花约 0.5 credits）。
/// 所以顺序是：先淘汰空闲过久的；都不过期时淘汰被匹配次数最少的，次数相同再淘汰最久未使用的。
fn pick_victim(baselines: &[Baseline], now: Instant) -> Option<usize> {
    baselines
        .iter()
        .enumerate()
        .min_by_key(|(_, b)| {
            let stale = now.saturating_duration_since(b.last_used) > STALE_AFTER;
            (!stale, if stale { 0 } else { b.hits }, b.last_used)
        })
        .map(|(i, _)| i)
}

static BASELINES: OnceLock<Mutex<HashMap<String, CacheEntry<Vec<Baseline>>>>> = OnceLock::new();

/// 与 history[0] 的 key 同口径：先规范化 `cch=`，避免计费哈希的逐轮变化被当成新块
fn block_fingerprint(block: &str) -> Fingerprint {
    // 只有含计费头 `cch=` 的块需要规范化（须拷贝一份来改写），其余直接借用，
    // 避免每次请求把整份系统提示多拷贝一遍
    if block.contains("cch=") {
        fingerprint(&normalize_billing_header(block.to_string()))
    } else {
        fingerprint(block)
    }
}

/// 基线是 `current` 的子多重集时，返回 `current` 中多出来的块的下标；否则返回 `None`
fn extras_of(baseline: &[Fingerprint], current: &[Fingerprint]) -> Option<Vec<usize>> {
    let mut remaining: Vec<Option<Fingerprint>> = baseline.iter().copied().map(Some).collect();
    let mut extras = Vec::new();
    for (i, fp) in current.iter().enumerate() {
        match remaining.iter_mut().find(|slot| **slot == Some(*fp)) {
            Some(slot) => *slot = None,
            None => extras.push(i),
        }
    }
    remaining.iter().all(Option::is_none).then_some(extras)
}

/// 把稳定系统块拆成 `(留在 history[0] 的块, 会话中途新增的块)`，两部分各自保持原顺序。
///
/// 会话首次出现（或没有任何基线匹配）时，全部块视为稳定，并登记为新基线。
pub(super) fn split_new_blocks<'a>(
    session_id: &str,
    blocks: &[&'a str],
) -> (Vec<&'a str>, Vec<&'a str>) {
    if blocks.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let fps: Vec<Fingerprint> = blocks.iter().map(|b| block_fingerprint(b)).collect();

    let map = BASELINES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    let entry = map
        .entry(session_id.to_string())
        .or_insert_with(|| CacheEntry::new(Vec::new()));
    entry.last_used = Instant::now();

    // 多个基线都匹配时取最大的（多出来的块最少）
    let best = entry
        .value
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let extras = extras_of(&b.fps, &fps)?;
            let extra_bytes: usize = extras.iter().map(|&k| fps[k].1).sum();
            (extra_bytes <= b.bytes).then_some((i, b.bytes, extras))
        })
        .max_by_key(|(_, bytes, _)| *bytes);

    if let Some((i, _, extras)) = best {
        entry.value[i].last_used = Instant::now();
        entry.value[i].hits = entry.value[i].hits.saturating_add(1);
        if extras.is_empty() {
            return (blocks.to_vec(), Vec::new());
        }
        let mut is_extra = vec![false; blocks.len()];
        for &k in &extras {
            is_extra[k] = true;
        }
        let mut stable = Vec::with_capacity(blocks.len() - extras.len());
        let mut added = Vec::with_capacity(extras.len());
        for (block, extra) in blocks.iter().zip(is_extra) {
            if extra {
                added.push(*block);
            } else {
                stable.push(*block);
            }
        }
        return (stable, added);
    }

    tracing::info!(
        session = %session_id,
        blocks = blocks.len(),
        known_baselines = entry.value.len(),
        "[baseline] 登记新的系统块基线"
    );
    if entry.value.len() >= MAX_BASELINES_PER_SESSION
        && let Some(victim) = pick_victim(&entry.value, Instant::now())
    {
        entry.value.swap_remove(victim);
    }
    entry.value.push(Baseline {
        bytes: fps.iter().map(|f| f.1).sum(),
        fps,
        last_used: Instant::now(),
        hits: 0,
    });
    evict_oldest_if_full(&mut map);
    (blocks.to_vec(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例用独立 session，避免全局表串扰
    fn sid(name: &str) -> String {
        format!("baseline-test-{name}")
    }

    fn big(tag: &str, n: usize) -> String {
        format!("{tag}:{}", "x".repeat(n))
    }

    #[test]
    fn first_request_registers_baseline_and_diverts_nothing() {
        let s = sid("first");
        let (a, b, c) = (big("a", 100), big("b", 100), big("c", 100));
        let (stable, added) = split_new_blocks(&s, &[&a, &b, &c]);
        assert_eq!(stable, [a.as_str(), b.as_str(), c.as_str()]);
        assert!(added.is_empty());
    }

    #[test]
    fn block_added_mid_session_is_diverted_wherever_it_lands() {
        let s = sid("added");
        let (a, b, c) = (big("a", 1000), big("b", 1000), big("c", 1000));
        let rule = big("Contents of rules", 200);
        split_new_blocks(&s, &[&a, &b, &c]);

        // 插在中间（与实测一致：新块不在末尾）
        let (stable, added) = split_new_blocks(&s, &[&a, &b, &rule, &c]);
        assert_eq!(stable, [a.as_str(), b.as_str(), c.as_str()]);
        assert_eq!(added, [rule.as_str()]);

        // 之后每轮都带着它：结果一致，history[0] 的输入不再变化
        let (stable2, added2) = split_new_blocks(&s, &[&a, &b, &rule, &c]);
        assert_eq!(stable2, stable);
        assert_eq!(added2, added);

        // 再来一个新块、顺序被打乱：仍只分流新块
        let rule2 = big("another", 150);
        let (stable3, added3) = split_new_blocks(&s, &[&rule2, &c, &a, &rule, &b]);
        assert_eq!(stable3, [c.as_str(), a.as_str(), b.as_str()]);
        assert_eq!(added3, [rule2.as_str(), rule.as_str()]);
    }

    #[test]
    fn rewritten_block_is_not_a_superset_and_becomes_new_baseline() {
        let s = sid("rewritten");
        let (a, b) = (big("a", 1000), big("b", 1000));
        let b2 = big("b-edited", 1000);
        split_new_blocks(&s, &[&a, &b]);

        let (stable, added) = split_new_blocks(&s, &[&a, &b2]);
        assert_eq!(stable, [a.as_str(), b2.as_str()]);
        assert!(added.is_empty());

        // 新基线生效：之后在它基础上新增的块被分流
        let rule = big("rule", 100);
        let (stable, added) = split_new_blocks(&s, &[&a, &b2, &rule]);
        assert_eq!(stable, [a.as_str(), b2.as_str()]);
        assert_eq!(added, [rule.as_str()]);
    }

    #[test]
    fn unrelated_request_does_not_evict_main_baseline() {
        let s = sid("subagent");
        let (a, b) = (big("a", 5000), big("b", 5000));
        let sub = big("subagent-system", 800);
        let rule = big("rule", 300);
        split_new_blocks(&s, &[&a, &b]);

        // 子代理请求：不包含主基线，原样放行并登记自己的基线
        let (stable, added) = split_new_blocks(&s, &[&sub]);
        assert_eq!(stable, [sub.as_str()]);
        assert!(added.is_empty());

        // 回到主请求，新增块照常被分流
        let (stable, added) = split_new_blocks(&s, &[&a, &b, &rule]);
        assert_eq!(stable, [a.as_str(), b.as_str()]);
        assert_eq!(added, [rule.as_str()]);
    }

    #[test]
    fn small_baseline_cannot_swallow_a_larger_main_prompt() {
        let s = sid("small-first");
        let ident = big("identity", 50);
        let main_prompt = big("main-system-prompt", 20_000);
        // 先到的是系统提示很短的辅助请求
        split_new_blocks(&s, &[&ident]);

        // 主请求包含辅助请求的块，但多出来的部分远大于基线：不能当成新增块搬走
        let (stable, added) = split_new_blocks(&s, &[&ident, &main_prompt]);
        assert_eq!(stable, [ident.as_str(), main_prompt.as_str()]);
        assert!(added.is_empty());
    }

    #[test]
    fn billing_header_cch_changes_are_not_new_blocks() {
        let s = sid("cch");
        let h1 = "x-anthropic-billing-header: cc_version=2.1; cch=11111;";
        let h2 = "x-anthropic-billing-header: cc_version=2.1; cch=22222;";
        let body = big("body", 500);
        split_new_blocks(&s, &[h1, &body]);
        let (stable, added) = split_new_blocks(&s, &[h2, &body]);
        assert_eq!(stable.len(), 2);
        assert!(added.is_empty());
    }

    #[test]
    fn duplicate_blocks_are_matched_by_count() {
        let s = sid("dup");
        let a = big("a", 1000);
        split_new_blocks(&s, &[&a]);
        // 基线里只有一份 a，请求里出现第二份相同内容的块，算新增
        let (stable, added) = split_new_blocks(&s, &[&a, &a]);
        assert_eq!(stable, [a.as_str()]);
        assert_eq!(added, [a.as_str()]);
    }

    #[test]
    fn subagent_burst_does_not_evict_frequently_used_main_baseline() {
        // 复现线上事件：主对话基线用了很多次，随后并行子代理在几十秒内连续登记
        // 多于上限的新基线；主对话回来时仍应命中自己的基线、新增块照常被分流
        let s = sid("burst");
        let (a, b) = (big("a", 5000), big("b", 5000));
        let rule = big("rule", 300);
        split_new_blocks(&s, &[&a, &b]);
        for _ in 0..5 {
            split_new_blocks(&s, &[&a, &b]);
        }

        for i in 0..(MAX_BASELINES_PER_SESSION + 2) {
            let sub = big(&format!("sub-{i}"), 800);
            split_new_blocks(&s, &[&sub]);
        }

        let (stable, added) = split_new_blocks(&s, &[&a, &b, &rule]);
        assert_eq!(stable, [a.as_str(), b.as_str()]);
        assert_eq!(added, [rule.as_str()]);
    }

    fn baseline_at(last_used: Instant, hits: u32) -> Baseline {
        Baseline {
            fps: Vec::new(),
            bytes: 0,
            last_used,
            hits,
        }
    }

    #[test]
    fn pick_victim_prefers_stale_then_fewest_hits_then_oldest() {
        let t0 = Instant::now();
        let at = |secs: u64| t0 + Duration::from_secs(secs);
        let now = at(100 * 60);

        // 过期的优先，即使它被匹配过很多次
        let list = [
            baseline_at(at(99 * 60), 1),
            baseline_at(at(10 * 60), 68),
            baseline_at(at(98 * 60), 0),
        ];
        assert_eq!(pick_victim(&list, now), Some(1));

        // 都没过期：次数最少的先走
        let list = [
            baseline_at(at(90 * 60), 40),
            baseline_at(at(99 * 60), 1),
            baseline_at(at(95 * 60), 5),
        ];
        assert_eq!(pick_victim(&list, now), Some(1));

        // 次数相同：最久未使用的先走
        let list = [baseline_at(at(98 * 60), 2), baseline_at(at(90 * 60), 2)];
        assert_eq!(pick_victim(&list, now), Some(1));

        // 多个都过期：最久未使用的先走
        let list = [baseline_at(at(20 * 60), 9), baseline_at(at(10 * 60), 1)];
        assert_eq!(pick_victim(&list, now), Some(1));

        assert_eq!(pick_victim(&[], now), None);
    }

    #[test]
    fn baselines_per_session_are_bounded() {
        let s = sid("bounded");
        for i in 0..(MAX_BASELINES_PER_SESSION + 3) {
            let blk = big(&format!("distinct-{i}"), 100);
            split_new_blocks(&s, &[&blk]);
        }
        let map = BASELINES.get().unwrap().lock().unwrap();
        assert!(map.get(&s).unwrap().value.len() <= MAX_BASELINES_PER_SESSION);
    }
}
