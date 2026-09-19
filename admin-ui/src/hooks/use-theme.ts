// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useState } from 'react'
import { flushSync } from 'react-dom'

export type Theme = 'light' | 'dark'

const STORAGE_KEY = 'admin-ui-theme'

function readStoredTheme(): Theme {
  return localStorage.getItem(STORAGE_KEY) === 'light' ? 'light' : 'dark'
}

function applyTheme(theme: Theme) {
  document.documentElement.classList.toggle('dark', theme === 'dark')
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
  const [theme, setTheme] = useState<Theme>(readStoredTheme)

  useEffect(() => {
    applyTheme(theme)
  }, [theme])

  const toggleTheme = useCallback((x?: number, y?: number) => {
    // 无动画能力/减弱动态时退化为直接切换
    if (!supportsCircularReveal()) {
      setTheme((prev) => {
        const next: Theme = prev === 'dark' ? 'light' : 'dark'
        localStorage.setItem(STORAGE_KEY, next)
        return next
      })
      return
    }
    // 圆形扩散：新主题自点击位置（缺省视口中心）向外揭示
    const cx = x ?? window.innerWidth / 2
    const cy = y ?? window.innerHeight / 2
    const radius = Math.hypot(Math.max(cx, window.innerWidth - cx), Math.max(cy, window.innerHeight - cy))
    document.startViewTransition(() => {
      // flushSync 强制同步渲染 + useEffect 切换 dark class，保证新快照已反映目标主题
      flushSync(() => {
        setTheme((prev) => {
          const next: Theme = prev === 'dark' ? 'light' : 'dark'
          localStorage.setItem(STORAGE_KEY, next)
          return next
        })
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
  }, [])

  return { theme, toggleTheme }
}
