// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useSyncExternalStore } from 'react'

const TICK_INTERVAL_MS = 30_000

let currentNow = Date.now()
let timer: ReturnType<typeof setInterval> | null = null
const listeners = new Set<() => void>()

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  if (timer === null) {
    currentNow = Date.now()
    timer = setInterval(() => {
      currentNow = Date.now()
      listeners.forEach((fn) => fn())
    }, TICK_INTERVAL_MS)
  }
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0 && timer !== null) {
      clearInterval(timer)
      timer = null
    }
  }
}

function getSnapshot(): number {
  return currentNow
}

/** 共享 30s 心跳时钟：所有订阅组件同步刷新，无订阅者时自动释放定时器 */
export function useNowTick(): number {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}
