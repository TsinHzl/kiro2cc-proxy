// Copyright (c) 2026 Harllan He. Licensed under MIT.

/** 生成前端本地使用的短 ID（消息、附件等），不保证全局唯一，仅用于列表 key 与本地引用 */
export function newId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}
