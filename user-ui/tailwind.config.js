/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        border: 'hsl(var(--border))',
        input: 'hsl(var(--input))',
        ring: 'hsl(var(--ring))',
        background: 'hsl(var(--background))',
        foreground: 'hsl(var(--foreground))',
        primary: {
          DEFAULT: 'hsl(var(--primary))',
          foreground: 'hsl(var(--primary-foreground))',
        },
        secondary: {
          DEFAULT: 'hsl(var(--secondary))',
          foreground: 'hsl(var(--secondary-foreground))',
        },
        destructive: {
          DEFAULT: 'hsl(var(--destructive))',
          foreground: 'hsl(var(--destructive-foreground))',
        },
        muted: {
          DEFAULT: 'hsl(var(--muted))',
          foreground: 'hsl(var(--muted-foreground))',
        },
        accent: {
          DEFAULT: 'hsl(var(--accent))',
          foreground: 'hsl(var(--accent-foreground))',
        },
        popover: {
          DEFAULT: 'hsl(var(--popover))',
          foreground: 'hsl(var(--popover-foreground))',
        },
        card: {
          DEFAULT: 'hsl(var(--card))',
          foreground: 'hsl(var(--card-foreground))',
        },
        // 设计稿原生令牌（design.md 决策 1 下半区）
        // 注意：这批令牌是整值 var() 引用，Tailwind 3.4 无法对其应用透明度修饰符
        //      —— `bg-brand/50` 会静默生成空规则（实测 0 条），不是降级而是无背景。
        //      需要半透明时一律使用 -soft / -line 变体，它们在 .dark 下已是 rgba。
        surface: {
          DEFAULT: 'var(--surface)',
          2: 'var(--surface-2)',
          3: 'var(--surface-3)',
        },
        sidebar: 'var(--sidebar)',
        hairline: {
          DEFAULT: 'var(--hairline)',
          2: 'var(--hairline-2)',
        },
        ink: {
          DEFAULT: 'var(--ink)',
          2: 'var(--ink-2)',
          3: 'var(--ink-3)',
        },
        brand: {
          DEFAULT: 'var(--brand)',
          hover: 'var(--brand-hover)',
          fg: 'var(--brand-fg)',
          soft: 'var(--brand-soft)',
          line: 'var(--brand-line)',
        },
        ok: {
          DEFAULT: 'var(--ok)',
          soft: 'var(--ok-soft)',
          line: 'var(--ok-line)',
        },
        warn: {
          DEFAULT: 'var(--warn)',
          soft: 'var(--warn-soft)',
          line: 'var(--warn-line)',
        },
        danger: {
          DEFAULT: 'var(--danger)',
          soft: 'var(--danger-soft)',
          line: 'var(--danger-line)',
        },
        track: 'var(--track)',
        'code-bg': 'var(--code-bg)',
        space: {
          950: '#09090b',
          900: '#0f0f11',
          850: '#121214',
          800: '#18181b',
          border: '#27272a',
        },
        neon: {
          purple: '#a855f7',
          cyan: '#06b6d4',
          green: '#22c55e',
          yellow: '#eab308',
          red: '#ef4444',
        },
      },
      borderRadius: {
        lg: 'var(--radius)',
        md: 'calc(var(--radius) - 2px)',
        sm: 'calc(var(--radius) - 4px)',
      },
      // 不覆盖 Tailwind 内置的 shadow-sm / shadow-md：其他 6 页仍在使用其默认值
      boxShadow: {
        hair: 'var(--shadow-sm)',
        panel: 'var(--shadow-md)',
        pop: 'var(--shadow-pop)',
      },
      fontFamily: {
        mono: ['"IBM Plex Mono"', '"JetBrains Mono"', 'Consolas', 'monospace'],
        sans: ['"IBM Plex Sans"', '"PingFang SC"', '"Hiragino Sans GB"', 'system-ui', 'sans-serif'],
      },
    },
  },
  plugins: [],
}
