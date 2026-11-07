// Copyright (c) 2026 Harllan He. Licensed under MIT.
import * as React from 'react'
import * as DropdownMenuPrimitive from '@radix-ui/react-dropdown-menu'
import { Check } from 'lucide-react'
import { cn } from '@/lib/utils'

const DropdownMenu = DropdownMenuPrimitive.Root
const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger

/** 菜单浮层：沿用面板令牌（surface 底 + hairline 描边 + shadow-pop） */
const DropdownMenuContent = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Content>
>(({ className, sideOffset = 6, align = 'end', ...props }, ref) => (
  <DropdownMenuPrimitive.Portal>
    <DropdownMenuPrimitive.Content
      ref={ref}
      sideOffset={sideOffset}
      align={align}
      className={cn(
        'z-50 min-w-[172px] overflow-hidden rounded-[8px] border border-transparent bg-surface p-1 shadow-pop',
        // 从触发点方向展开：origin 取 Radix 注入的变量
        'origin-[var(--radix-dropdown-menu-content-transform-origin)] data-[state=open]:animate-popover-in data-[state=closed]:animate-popover-out motion-reduce:data-[state=open]:animate-none motion-reduce:data-[state=closed]:animate-none',
        className,
      )}
      {...props}
    />
  </DropdownMenuPrimitive.Portal>
))
DropdownMenuContent.displayName = DropdownMenuPrimitive.Content.displayName

/** 菜单项；danger 用于删除等破坏性操作 */
const DropdownMenuItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Item> & { danger?: boolean }
>(({ className, danger = false, ...props }, ref) => (
  <DropdownMenuPrimitive.Item
    ref={ref}
    className={cn(
      'flex cursor-pointer select-none items-center gap-2 rounded-[5px] px-2 py-[6px] text-[11.5px] font-medium outline-none transition-colors',
      'data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
      danger
        ? 'text-danger data-[highlighted]:bg-danger-soft'
        : 'text-ink-2 data-[highlighted]:bg-surface-3 data-[highlighted]:text-ink',
      className,
    )}
    {...props}
  />
))
DropdownMenuItem.displayName = DropdownMenuPrimitive.Item.displayName

const DropdownMenuSeparator = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Separator>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Separator>
>(({ className, ...props }, ref) => (
  <DropdownMenuPrimitive.Separator ref={ref} className={cn('-mx-1 my-1 h-[0.5px] bg-hairline', className)} {...props} />
))
DropdownMenuSeparator.displayName = DropdownMenuPrimitive.Separator.displayName

/** 单选组 + 单选项：选中项左侧渲染选中态圆点，样式对齐 DropdownMenuItem */
const DropdownMenuRadioGroup = DropdownMenuPrimitive.RadioGroup

const DropdownMenuRadioItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.RadioItem>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.RadioItem>
>(({ className, children, ...props }, ref) => (
  <DropdownMenuPrimitive.RadioItem
    ref={ref}
    className={cn(
      'flex cursor-pointer select-none items-center gap-2 rounded-[5px] px-2 py-[6px] text-[11.5px] font-medium outline-none transition-colors',
      'data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
      'text-ink data-[highlighted]:bg-surface-3 data-[highlighted]:text-ink data-[state=checked]:bg-brand-soft data-[state=checked]:font-semibold data-[highlighted]:data-[state=checked]:bg-surface-3',
      className,
    )}
    {...props}
  >
    {/* 选中标记：内层必须是块级/图标元素，行内 span 会让 size 失效导致圆点不可见 */}
    <span className="flex size-3.5 flex-none items-center justify-center">
      <DropdownMenuPrimitive.ItemIndicator>
        <Check className="size-3.5 text-brand" strokeWidth={2.5} />
      </DropdownMenuPrimitive.ItemIndicator>
    </span>
    {children}
  </DropdownMenuPrimitive.RadioItem>
))
DropdownMenuRadioItem.displayName = DropdownMenuPrimitive.RadioItem.displayName

export {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
}
