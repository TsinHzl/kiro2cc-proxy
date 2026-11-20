// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useSyncExternalStore } from 'react'
import { flushSync } from 'react-dom'
import { COLOR_THEMES, type ColorTheme } from '@/theme-definitions'

/** 外观模式：light/dark 为显式选择 */
export type ThemeMode = 'light' | 'dark'
/** 解析后的实际明暗（当前与 ThemeMode 一致，保留以便后续扩展） */
export type Theme = 'light' | 'dark'

const MODE_STORAGE_KEY = 'admin-ui-theme-mode'
const COLOR_STORAGE_KEY = 'admin-ui-color-theme'
const GRADIENT_STORAGE_KEY = 'admin-ui-canvas-gradient'
/** 旧版单值 key（存 light/dark），升级后迁移到 MODE_STORAGE_KEY */
const LEGACY_STORAGE_KEY = 'admin-ui-theme'

const DEFAULT_MODE: ThemeMode = 'light'
/** 开箱默认色彩主题：Spotify 律动绿 */
const DEFAULT_COLOR_THEME: ColorTheme = 'spotify'
/** 开箱默认关闭内容区渐变色 */
const DEFAULT_CANVAS_GRADIENT = false

function readStoredMode(): ThemeMode {
  try {
    const stored = localStorage.getItem(MODE_STORAGE_KEY)
    if (stored === 'light' || stored === 'dark') return stored
    // 旧版本仅存 light/dark，视为显式选择迁移
    const legacy = localStorage.getItem(LEGACY_STORAGE_KEY)
    if (legacy === 'light' || legacy === 'dark') return legacy
  } catch {
    /* localStorage 不可用（隐私模式等），走默认值 */
  }
  return DEFAULT_MODE
}

function readStoredColorTheme(): ColorTheme {
  try {
    const stored = localStorage.getItem(COLOR_STORAGE_KEY)
    if (stored && COLOR_THEMES.some((item) => item.id === stored)) return stored as ColorTheme
  } catch {
    /* 同上 */
  }
  return DEFAULT_COLOR_THEME
}

function applyTheme(mode: ThemeMode) {
  document.documentElement.classList.toggle('dark', mode === 'dark')
}

function readStoredCanvasGradient(): boolean {
  try {
    return localStorage.getItem(GRADIENT_STORAGE_KEY) === 'true'
  } catch {
    return DEFAULT_CANVAS_GRADIENT
  }
}

function applyColorTheme(id: ColorTheme) {
  const root = document.documentElement
  COLOR_THEMES.forEach((item) => {
    root.classList.remove(`theme-${item.id}`)
  })
  root.classList.add(`theme-${id}`)
}

function applyCanvasGradient(enabled: boolean) {
  document.documentElement.classList.toggle('canvas-gradient', enabled)
}

/** View Transitions 圆形扩散动画是否可用（浏览器支持 + 未开启减弱动态效果） */
function supportsCircularReveal() {
  return (
    typeof document !== 'undefined' &&
    'startViewTransition' in document &&
    !window.matchMedia('(prefers-reduced-motion: reduce)').matches
  )
}

interface ThemeSnapshot {
  themeMode: ThemeMode
  colorTheme: ColorTheme
  canvasGradient: boolean
}

const STORAGE_KEYS = new Set([MODE_STORAGE_KEY, LEGACY_STORAGE_KEY, COLOR_STORAGE_KEY, GRADIENT_STORAGE_KEY])

function readSnapshot(): ThemeSnapshot {
  return {
    themeMode: readStoredMode(),
    colorTheme: readStoredColorTheme(),
    canvasGradient: readStoredCanvasGradient(),
  }
}

function applyAllThemeState(state: ThemeSnapshot) {
  if (typeof document === 'undefined') return
  applyTheme(state.themeMode)
  applyColorTheme(state.colorTheme)
  applyCanvasGradient(state.canvasGradient)
}

function sameSnapshot(a: ThemeSnapshot, b: ThemeSnapshot): boolean {
  return a.themeMode === b.themeMode && a.colorTheme === b.colorTheme && a.canvasGradient === b.canvasGradient
}

// 模块级共享状态：保证主题设置页、侧边栏切换按钮等多个 useTheme() 调用方始终同步
let themeSnapshot: ThemeSnapshot = readSnapshot()
const listeners = new Set<() => void>()

function updateThemeSnapshot(next: ThemeSnapshot) {
  if (sameSnapshot(themeSnapshot, next)) return
  themeSnapshot = next
  applyAllThemeState(next)
  listeners.forEach((listener) => listener())
}

function handleStorage(e: StorageEvent) {
  if (e.key !== null && !STORAGE_KEYS.has(e.key)) return
  updateThemeSnapshot(readSnapshot())
}

function subscribe(listener: () => void) {
  if (listeners.size === 0 && typeof window !== 'undefined') {
    window.addEventListener('storage', handleStorage)
  }
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0 && typeof window !== 'undefined') {
      window.removeEventListener('storage', handleStorage)
    }
  }
}

function getSnapshot(): ThemeSnapshot {
  return themeSnapshot
}

export function useTheme() {
  const { themeMode, colorTheme, canvasGradient } = useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
  /** 解析后的实际明暗（当前与 themeMode 等价，直接派生避免双 state 失同步） */
  const theme: Theme = themeMode

  useEffect(() => {
    applyAllThemeState({ themeMode, colorTheme, canvasGradient })
  }, [themeMode, colorTheme, canvasGradient])

  /** 内部切模式：持久化 + 同步全局快照，动画外壳由调用方决定 */
  const commitMode = useCallback((mode: ThemeMode) => {
    try {
      localStorage.setItem(MODE_STORAGE_KEY, mode)
      // 迁移完成后清掉旧 key，避免下次启动重复迁移判断
      localStorage.removeItem(LEGACY_STORAGE_KEY)
    } catch {
      /* localStorage 不可用时仅内存生效 */
    }
    updateThemeSnapshot({ ...themeSnapshot, themeMode: mode })
  }, [])

  const setThemeMode = useCallback(
    (mode: ThemeMode, x?: number, y?: number) => {
      if (mode === themeSnapshot.themeMode) return
      if (!supportsCircularReveal()) {
        commitMode(mode)
        return
      }
      const cx = x ?? window.innerWidth / 2
      const cy = y ?? window.innerHeight / 2
      const radius = Math.hypot(Math.max(cx, window.innerWidth - cx), Math.max(cy, window.innerHeight - cy))
      document.startViewTransition(() => {
        // flushSync 强制同步渲染，保证所有订阅组件与根节点 class 同时反映目标主题
        flushSync(() => {
          commitMode(mode)
        })
      }).ready.then(() => {
        document.documentElement.animate(
          { clipPath: [`circle(0px at ${cx}px ${cy}px)`, `circle(${radius}px at ${cx}px ${cy}px)`] },
          {
            duration: 550,
            easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
            pseudoElement: '::view-transition-new(root)',
          },
        )
      }).catch(() => {
        // 动画被新 transition 抢占（连续快速点击）时 ready 会 reject，静默即可
      })
    },
    [commitMode],
  )

  const setColorTheme = useCallback((id: ColorTheme) => {
    try {
      localStorage.setItem(COLOR_STORAGE_KEY, id)
    } catch {
      /* localStorage 不可用时仅内存生效 */
    }
    updateThemeSnapshot({ ...themeSnapshot, colorTheme: id })
  }, [])

  const setCanvasGradient = useCallback((enabled: boolean) => {
    try {
      localStorage.setItem(GRADIENT_STORAGE_KEY, String(enabled))
    } catch {
      /* localStorage 不可用时仅内存生效 */
    }
    updateThemeSnapshot({ ...themeSnapshot, canvasGradient: enabled })
  }, [])

  const toggleTheme = useCallback(
    (x?: number, y?: number) => {
      setThemeMode(themeSnapshot.themeMode === 'dark' ? 'light' : 'dark', x, y)
    },
    [setThemeMode],
  )

  const resetTheme = useCallback(() => {
    // 恢复为出厂默认：浅色 + Spotify 律动绿 + 关闭画布渐变
    try {
      localStorage.removeItem(MODE_STORAGE_KEY)
      localStorage.removeItem(LEGACY_STORAGE_KEY)
      localStorage.removeItem(COLOR_STORAGE_KEY)
      localStorage.removeItem(GRADIENT_STORAGE_KEY)
    } catch {
      /* localStorage 不可用时仅内存生效 */
    }
    updateThemeSnapshot({
      themeMode: DEFAULT_MODE,
      colorTheme: DEFAULT_COLOR_THEME,
      canvasGradient: DEFAULT_CANVAS_GRADIENT,
    })
  }, [])

  return {
    theme,
    themeMode,
    colorTheme,
    canvasGradient,
    setThemeMode,
    setColorTheme,
    setCanvasGradient,
    toggleTheme,
    resetTheme,
  }
}
