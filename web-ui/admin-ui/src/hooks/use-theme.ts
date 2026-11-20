// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useState } from 'react'
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

export function useTheme() {
  const [themeMode, setThemeModeState] = useState<ThemeMode>(readStoredMode)
  /** 解析后的实际明暗（当前与 themeMode 等价，直接派生避免双 state 失同步） */
  const theme: Theme = themeMode
  const [colorTheme, setColorThemeState] = useState<ColorTheme>(readStoredColorTheme)
  const [canvasGradient, setCanvasGradientState] = useState<boolean>(readStoredCanvasGradient)

  useEffect(() => {
    applyTheme(themeMode)
  }, [themeMode])

  useEffect(() => {
    applyColorTheme(colorTheme)
  }, [colorTheme])

  useEffect(() => {
    applyCanvasGradient(canvasGradient)
  }, [canvasGradient])

  /** 内部切模式：持久化 + 同步解析后的明暗，动画外壳由调用方决定 */
  const commitMode = useCallback((mode: ThemeMode) => {
    try {
      localStorage.setItem(MODE_STORAGE_KEY, mode)
      // 迁移完成后清掉旧 key，避免下次启动重复迁移判断
      localStorage.removeItem(LEGACY_STORAGE_KEY)
    } catch {
      /* localStorage 不可用时仅内存生效 */
    }
    setThemeModeState(mode)
  }, [])

  const setThemeMode = useCallback(
    (mode: ThemeMode, x?: number, y?: number) => {
      if (!supportsCircularReveal()) {
        commitMode(mode)
        return
      }
      const cx = x ?? window.innerWidth / 2
      const cy = y ?? window.innerHeight / 2
      const radius = Math.hypot(Math.max(cx, window.innerWidth - cx), Math.max(cy, window.innerHeight - cy))
      document.startViewTransition(() => {
        // flushSync 强制同步渲染 + useEffect 切换 class，保证新快照已反映目标主题
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
    setColorThemeState(id)
  }, [])

  const setCanvasGradient = useCallback((enabled: boolean) => {
    try {
      localStorage.setItem(GRADIENT_STORAGE_KEY, String(enabled))
    } catch {
      /* localStorage 不可用时仅内存生效 */
    }
    setCanvasGradientState(enabled)
  }, [])

  const toggleTheme = useCallback(
    (x?: number, y?: number) => {
      setThemeMode(theme === 'dark' ? 'light' : 'dark', x, y)
    },
    [setThemeMode, theme],
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
    setThemeModeState(DEFAULT_MODE)
    setColorThemeState(DEFAULT_COLOR_THEME)
    setCanvasGradientState(DEFAULT_CANVAS_GRADIENT)
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
