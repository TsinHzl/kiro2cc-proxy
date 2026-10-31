// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 主题预览区文字色：色板只提供装饰色，文字色按实际底色计算，保证 WCAG AA（4.5:1）。

const AA = 4.5
const DARK_FALLBACK = '#0F1113'
const LIGHT_FALLBACK = '#FFFFFF'
const SECONDARY_ON_LIGHT = '#5F646B'
const SECONDARY_ON_DARK = '#9AA1A9'

type Rgb = [number, number, number]

function hslToRgb(h: number, s: number, l: number): Rgb {
  const sat = s / 100
  const lig = l / 100
  const a = sat * Math.min(lig, 1 - lig)
  const f = (n: number) => {
    const k = (n + h / 30) % 12
    return lig - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))
  }
  return [Math.round(f(0) * 255), Math.round(f(8) * 255), Math.round(f(4) * 255)]
}

function rgbToHsl([r, g, b]: Rgb): [number, number, number] {
  const rn = r / 255
  const gn = g / 255
  const bn = b / 255
  const max = Math.max(rn, gn, bn)
  const min = Math.min(rn, gn, bn)
  const l = (max + min) / 2
  const d = max - min
  if (d === 0) return [0, 0, l * 100]
  const s = d / (1 - Math.abs(2 * l - 1))
  let h: number
  if (max === rn) h = ((gn - bn) / d) % 6
  else if (max === gn) h = (bn - rn) / d + 2
  else h = (rn - gn) / d + 4
  return [(h * 60 + 360) % 360, s * 100, l * 100]
}

function parseColor(color: string): Rgb | null {
  const hex = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color)
  if (hex) return [parseInt(hex[1], 16), parseInt(hex[2], 16), parseInt(hex[3], 16)]
  const hsl = /^hsl\(\s*([\d.]+)\s+([\d.]+)%\s+([\d.]+)%\s*\)$/i.exec(color)
  if (hsl) return hslToRgb(Number(hsl[1]), Number(hsl[2]), Number(hsl[3]))
  return null
}

function luminance([r, g, b]: Rgb): number {
  const lin = (v: number) => {
    const c = v / 255
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

const toHex = ([r, g, b]: Rgb) => `#${[r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('')}`

/** preferred 对 background 不足 4.5:1 时，保持色相与饱和度、沿远离背景方向逐 1% 调明度；到极值仍不够则退回黑/白。解析失败原样返回。 */
export function readableText(preferred: string, background: string): string {
  const fg = parseColor(preferred)
  const bg = parseColor(background)
  if (!fg || !bg) return preferred
  if (contrast(fg, bg) >= AA) return preferred

  const step = luminance(bg) > 0.179 ? -1 : 1
  const [h, s, l0] = rgbToHsl(fg)
  for (let l = Math.round(l0) + step; l >= 0 && l <= 100; l += step) {
    const cand = hslToRgb(h, s, l)
    if (contrast(cand, bg) >= AA) return toHex(cand)
  }
  const dark = parseColor(DARK_FALLBACK)!
  const light = parseColor(LIGHT_FALLBACK)!
  return contrast(dark, bg) >= contrast(light, bg) ? DARK_FALLBACK : LIGHT_FALLBACK
}

/** 次要文字：背景偏亮取深灰，偏暗取浅灰（相对亮度阈值 0.4），再过 readableText 兜底。 */
export function secondaryText(background: string): string {
  const bg = parseColor(background)
  const preferred = bg && luminance(bg) < 0.4 ? SECONDARY_ON_DARK : SECONDARY_ON_LIGHT
  return readableText(preferred, background)
}
