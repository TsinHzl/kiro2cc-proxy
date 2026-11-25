// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useEffect, useState } from 'react'

/**
 * 监听视口纵向滚动是否离开顶部，配合 `.bg-sticky-canvas[data-scrolled]` 实现
 * 「未滚动时全透明融于极光，滚动后切为视口同源极光实底防止正文透字」。
 */
export function useWindowScrolled(): boolean {
  const [scrolled, setScrolled] = useState(() =>
    typeof window !== 'undefined' && window.scrollY > 0,
  )

  useEffect(() => {
    const sync = () => setScrolled(window.scrollY > 0)
    sync()
    window.addEventListener('scroll', sync, { passive: true })
    return () => window.removeEventListener('scroll', sync)
  }, [])

  return scrolled
}
