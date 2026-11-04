// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { ModelItem } from '@/types/api'

/** 自动路由模型的固定 id（后端 guess_owned_by 将其归为 kiro） */
export const AUTO_MODEL_ID = 'auto'
/** 家族哨兵：仅这两个特例需经 i18n 映射，真实族名（如 `Claude 4.x`）直接展示 */
export const FAMILY_AUTO = '@@auto'
export const FAMILY_OTHER = '@@other'

/** 新命名：族名在前（claude-opus-4-1-…），主版本即一代 */
export const MODERN_CLAUDE = /^claude-(?:opus|sonnet|haiku)-(\d+)/
/** 旧命名：版本在前（claude-3-7-sonnet-…），主次版本合起来才是一代 */
export const LEGACY_CLAUDE = /^claude-(\d+)(?:-(\d+))?-/

/**
 * 从模型 id 推导世代家族（spec：家族由 id 前缀客户端推导，无法归类的归入「其他」）。
 *
 * 两代 Anthropic 命名的分组口径不同 —— 新命名归「Claude 4.x」，旧命名归「Claude 3.7」，
 * 与设计稿的分组示例一致。非 claude 模型退化为提供方名。
 */
export function modelFamily(model: ModelItem): string {
  const id = model.id.toLowerCase()
  if (id === AUTO_MODEL_ID) return FAMILY_AUTO
  const modern = MODERN_CLAUDE.exec(id)
  if (modern) return `Claude ${modern[1]}.x`
  const legacy = LEGACY_CLAUDE.exec(id)
  if (legacy) return legacy[2] ? `Claude ${legacy[1]}.${legacy[2]}` : `Claude ${legacy[1]}`
  if (id.startsWith('claude')) return 'Claude'
  return model.owned_by !== '' && model.owned_by !== 'unknown' ? model.owned_by.toUpperCase() : FAMILY_OTHER
}

/**
 * 品牌口径家族：Claude 全世代合为一族，供统计卡片计数。
 * 与 modelFamily 的代际口径互补 —— 表格/弹窗分组仍按世代拆分以展示各代倍率区间。
 */
export function familyBrand(model: ModelItem): string {
  const id = model.id.toLowerCase()
  if (id === AUTO_MODEL_ID) return FAMILY_AUTO
  if (MODERN_CLAUDE.test(id) || LEGACY_CLAUDE.test(id) || id.startsWith('claude')) return 'Claude'
  return model.owned_by !== '' && model.owned_by !== 'unknown' ? model.owned_by.toUpperCase() : FAMILY_OTHER
}

/** 按任意家族口径函数分组；组序与组内序均沿用后端返回顺序（Map 保插入序） */
export function groupByFamilyWith(models: ModelItem[], family: (m: ModelItem) => string): FamilyGroup[] {
  const byName = new Map<string, ModelItem[]>()
  for (const model of models) {
    const name = family(model)
    byName.set(name, [...(byName.get(name) ?? []), model])
  }
  return [...byName].map(([name, items]) => ({ name, models: items }))
}

export interface FamilyGroup {
  name: string
  models: ModelItem[]
}

/** 按推导家族分组；组序与组内序均沿用后端返回顺序（Map 保插入序） */
export function groupByFamily(models: ModelItem[]): FamilyGroup[] {
  const byName = new Map<string, ModelItem[]>()
  for (const model of models) {
    const name = modelFamily(model)
    byName.set(name, [...(byName.get(name) ?? []), model])
  }
  return [...byName].map(([name, items]) => ({ name, models: items }))
}

export type InputModality = 'text' | 'image'

/**
 * 上游取值 → 归一化模态：精确匹配（忽略大小写与空白），未知值忽略。
 * 用 Map 而非 Record 查表 —— Record 的索引会穿透原型链，`"constructor"` 这类键会取到
 * 函数并被误当成已知模态写进结果集（最终被顺序表过滤掉，但类型断言失真）。
 */
const KNOWN_MODALITY = new Map<string, InputModality>([
  ['text', 'text'],
  ['image', 'image'],
])
/** 固定输出顺序，保证列表页与弹窗的徽章排列一致，不随上游返回顺序漂移 */
const MODALITY_ORDER: readonly InputModality[] = ['text', 'image']

/** 上游 `supportedInputTypes`（如 `["TEXT","IMAGE"]`）→ 归一化模态集合；未识别的值忽略 */
export function inputModalities(inputTypes?: string[] | null): InputModality[] {
  const found = new Set<InputModality>()
  for (const item of inputTypes ?? []) {
    const modality = KNOWN_MODALITY.get(item.trim().toLowerCase())
    if (modality) found.add(modality)
  }
  return MODALITY_ORDER.filter((m) => found.has(m))
}

/** 空态语义：未声明（含静态表回退）/ 声明了但本项目一个都不认识 */
export type ModalityEmptyState =
  | { kind: 'none' }
  | { kind: 'unrecognized'; values: string }

/**
 * 模态为空时的语义派生 —— 两种空态的文案与占位符不同，列表页与弹窗共用本函数，
 * 避免「未返回」与「不认识」的口径在两处各自演化。
 */
export function modalityEmptyState(inputTypes?: string[] | null): ModalityEmptyState {
  const declared = (inputTypes?.length ?? 0) > 0
  return declared
    ? { kind: 'unrecognized', values: (inputTypes ?? []).join(', ') }
    : { kind: 'none' }
}

export interface RateRange {
  min: number
  max: number
}

/** 倍率区间；全部为 null 时返回 null（spec：不得渲染成 NaN） */
export function rateRange(models: ModelItem[]): RateRange | null {
  const rates = models.map((m) => m.rate_multiplier).filter((r): r is number => r != null)
  if (rates.length === 0) return null
  return { min: Math.min(...rates), max: Math.max(...rates) }
}

export function formatRate(rate: number): string {
  return `${rate.toFixed(2)}×`
}

export function formatRateRange(range: RateRange | null): string {
  if (range === null) return '—'
  return range.min === range.max ? formatRate(range.min) : `${formatRate(range.min)} － ${formatRate(range.max)}`
}
