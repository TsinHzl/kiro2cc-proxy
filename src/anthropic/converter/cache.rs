// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 会话级 history[0] 冻结缓存基建

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub(super) const SESSION_CACHE_CAPACITY: usize = 1024;

#[derive(Clone)]
pub(super) struct CacheEntry<T: Clone> {
    pub(super) value: T,
    pub(super) last_used: Instant,
}

impl<T: Clone> CacheEntry<T> {
    pub(super) fn new(value: T) -> Self {
        Self {
            value,
            last_used: Instant::now(),
        }
    }
}

pub(super) fn evict_oldest_if_full<T: Clone>(map: &mut HashMap<String, CacheEntry<T>>) {
    while map.len() > SESSION_CACHE_CAPACITY {
        // 优化：全局 Mutex 锁内避免 O(N) 扫描，使用 HashMap 迭代器提供的 O(1) 伪随机键进行淘汰
        let Some(random_key) = map.keys().next().cloned() else {
            break;
        };
        map.remove(&random_key);
    }
}

pub(super) static PREV_H0: OnceLock<Mutex<HashMap<String, CacheEntry<String>>>> = OnceLock::new();
