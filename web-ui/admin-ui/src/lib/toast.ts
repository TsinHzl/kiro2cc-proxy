// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 全局 toast 默认 2000ms（App.tsx）；错误提示常带后端错误原文，默认延长到 5000ms，调用方传 duration 时以调用方为准
import { toast as sonnerToast, type ExternalToast } from 'sonner'

const ERROR_DURATION = 5000

export const toast = {
  ...sonnerToast,
  error: (message: Parameters<typeof sonnerToast.error>[0], data?: ExternalToast) =>
    sonnerToast.error(message, { duration: ERROR_DURATION, ...data }),
}
