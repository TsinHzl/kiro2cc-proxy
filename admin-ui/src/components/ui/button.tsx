import * as React from 'react'
import { Slot } from '@radix-ui/react-slot'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const buttonVariants = cva(
  // 设计稿 .btn 基类：31px 高 / 7px 圆角 / 12.5px 字号；阴影随变体（.btn-ghost 明确 box-shadow:none）
  // svg 取色（.btn svg / .btn:hover svg）也属基类；反色变体自带 [&_svg]:* 覆盖（tailwind-merge 后者胜）
  'inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-[7px] border text-[12.5px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-[15px] [&_svg]:shrink-0 [&_svg]:text-ink-3 hover:[&_svg]:text-ink-2',
  {
    variants: {
      variant: {
        /** .btn-primary */
        default:
          'border-transparent bg-brand font-semibold text-brand-fg shadow-hair [&_svg]:text-brand-fg [&_svg]:opacity-90 hover:bg-brand-hover hover:[&_svg]:text-brand-fg',
        /** .btn-danger */
        destructive:
          'border-danger-line bg-surface text-danger shadow-hair [&_svg]:text-danger hover:border-danger hover:bg-danger-soft hover:[&_svg]:text-danger',
        /** .btn 常规态 */
        outline:
          'border-hairline-2 bg-surface text-ink-2 shadow-hair hover:border-ink-3 hover:bg-surface-2 hover:text-ink',
        /** 次级：常规态去阴影 + surface-3 底（设计稿无对应档，按令牌体系推导） */
        secondary: 'border-transparent bg-surface-3 text-ink-2 hover:bg-hairline hover:text-ink',
        /** .btn-ghost */
        ghost: 'border-transparent bg-transparent text-ink-2 hover:bg-surface-3 hover:text-ink',
        link: 'border-transparent text-brand underline-offset-4 hover:underline',
      },
      size: {
        default: 'h-[31px] px-[11px]',
        /** .btn-sm */
        sm: 'h-[26px] px-2 text-[11.5px]',
        lg: 'h-9 rounded-lg px-5 text-[13px]',
        /** .btn-icon */
        icon: 'h-[31px] w-[31px] p-0',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button'
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    )
  }
)
Button.displayName = 'Button'

export { Button, buttonVariants }
