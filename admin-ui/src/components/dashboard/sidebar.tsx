// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 侧边栏区块（自 dashboard.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { LogOut, Server, PanelLeftClose, PanelLeftOpen, Sun, Moon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { ADMIN_NAME } from '@/components/dashboard/panel-constants'

export interface NavItem {
  key: string
  label: string
  icon: typeof Server
  count: number | undefined
  active: boolean
  onClick: () => void
}

interface SidebarProps {
  sidebarCollapsed: boolean
  sidebarContentCollapsed: boolean
  sidebarContentFading: boolean
  navGroups: { title: string; items: NavItem[] }[]
  serverInfo: { version: string } | undefined
  serverInfoError: boolean
  theme: string
  toggleSidebarCollapsed: () => void
  handleLogout: () => void
  toggleTheme: (x: number, y: number) => void
}

export function Sidebar({
  sidebarCollapsed,
  sidebarContentCollapsed,
  sidebarContentFading,
  navGroups,
  serverInfo,
  serverInfoError,
  theme,
  toggleSidebarCollapsed,
  handleLogout,
  toggleTheme,
}: SidebarProps) {
  const { t } = useTranslation()

  // 页脚状态点：加载中 / 请求失败时 serverInfo 为 undefined；已有缓存后端再断开则由 isError 兜底转灰
  const serverHealthy = !!serverInfo?.version && !serverInfoError
  const serverStatusLabel = serverHealthy
    ? `kiro2cc-proxy v${serverInfo.version} · ${t('dashboard.serviceRunning')}`
    : t('dashboard.serviceUnknown')

  return (
      <aside className={`${sidebarCollapsed ? 'w-16' : 'w-[232px]'} bg-sidebar bg-grid-dot border-r border-hairline fixed top-0 left-0 bottom-0 flex flex-col z-10 transition-all duration-200`}>
        {/* 内容区整体做一次透明度过渡：把 header/nav/footer 所有跟随收起态瞬时切换的布局
            （flex 方向、文字显隐、ml-auto）都藏在这次淡出淡入的不可见瞬间，避免逐处单独处理时互相错位 */}
        <div className={`flex h-full flex-col transition-opacity duration-100 ${sidebarContentFading ? 'opacity-0' : 'opacity-100'}`}>
        <div className={`flex items-center border-b border-hairline ${sidebarContentCollapsed ? 'flex-col gap-2 px-2 py-3' : 'gap-2.5 px-4 pt-4 pb-3.5'}`}>
          <a
            href="https://github.com/TsinHzl/kiro2cc-proxy"
            target="_blank"
            rel="noopener noreferrer"
            className={`flex items-center group min-w-0 ${sidebarContentCollapsed ? '' : 'gap-2.5'}`}
          >
            {/* 方案 4（Aurora Prism）图标自带圆角底座与极光边框，故不再套品牌渐变方块；
                随主题切换 dark / light 两版，与设计稿 preview.html 的实机模拟一致 */}
            {/* 必须走 BASE_URL 拼接：vite 的 base('/admin/') 只重写 index.html 的
                href/src，不改 TS 源码字符串，写死 "/logo-*.svg" 会打到根路径 404 */}
            <img
              src={`${import.meta.env.BASE_URL}logo-aurora-dark.svg`}
              alt="Kiro2CCProxy"
              className="hidden h-[30px] w-[30px] shrink-0 rounded-[7px] shadow-hair dark:block"
            />
            <img
              src={`${import.meta.env.BASE_URL}logo-aurora-light.svg`}
              alt="Kiro2CCProxy"
              className="h-[30px] w-[30px] shrink-0 rounded-[7px] shadow-hair dark:hidden"
            />
            {!sidebarContentCollapsed && (
              <div className="min-w-0">
                <div className="text-[13.5px] font-semibold leading-[1.2] tracking-[-.01em] group-hover:text-brand transition-colors">Kiro2CCProxy</div>
                <div className="text-[10.5px] tracking-[.02em] text-ink-3 group-hover:text-brand transition-colors">{t('dashboard.consoleSubtitle')}</div>
              </div>
            )}
          </a>
          <Button
            variant="ghost"
            size="icon"
            className={`h-7 w-7 shrink-0 text-ink-3 hover:bg-surface-3 hover:text-ink-2 ${sidebarContentCollapsed ? '' : 'ml-auto'}`}
            onClick={toggleSidebarCollapsed}
            title={sidebarCollapsed ? t('dashboard.expandSidebar') : t('dashboard.collapseSidebar')}
            aria-label={sidebarCollapsed ? t('dashboard.expandSidebar') : t('dashboard.collapseSidebar')}
          >
            {sidebarCollapsed ? <PanelLeftOpen className="h-3.5 w-3.5" /> : <PanelLeftClose className="h-3.5 w-3.5" />}
          </Button>
        </div>
        <nav className="flex-1 overflow-y-auto px-2 py-3">
          <TooltipProvider delayDuration={200}>
            {navGroups.map((group, groupIndex) => (
              <div key={group.title}>
                {sidebarContentCollapsed ? (
                  /* 设计稿 .shell.is-collapsed .nav-group：标题降级为 1px 分隔线，首组不渲染 */
                  groupIndex > 0 && (
                    <div role="separator" aria-label={group.title} className="mx-[14px] my-[9px] h-px bg-hairline-2" />
                  )
                ) : (
                  <div className={`px-2.5 pb-1.5 text-[10px] font-semibold uppercase tracking-[.09em] text-ink-3 ${groupIndex === 0 ? 'pt-0.5' : 'pt-3'}`}>
                    {group.title}
                  </div>
                )}
                {group.items.map(({ key, label, icon: Icon, count, active, onClick }) => {
                  // tooltip 与 aria-label 同源：收起态文案 / 计数被隐藏，可访问名称仍完整
                  const fullLabel = count === undefined ? label : `${label} · ${count}`
                  const item = (
                    <button
                      key={key}
                      onClick={onClick}
                      aria-label={fullLabel}
                      aria-current={active ? 'true' : undefined}
                      className={`relative mb-0.5 flex h-[33px] w-full items-center rounded-[7px] text-[12.5px] transition-colors ${sidebarContentCollapsed ? 'justify-center' : 'gap-[9px] px-2.5'} ${active ? 'bg-brand-soft font-semibold text-brand' : 'font-[450] text-ink-2 hover:bg-surface-3 hover:text-ink'}`}
                    >
                      {active && (
                        /* -left-2 与 <nav> 的 px-2 数值耦合：竖条要贴在侧栏左边缘（padding-box x=0），
                           两处必须同步；展开态与 64px 收起态共用此几何，对齐设计稿 .nav-item::before{left:-8px} */
                        <span className="absolute -left-2 top-2 bottom-2 w-[2.5px] rounded-r-[3px] bg-brand" aria-hidden="true" />
                      )}
                      <Icon className="w-4 h-4 shrink-0" />
                      {!sidebarContentCollapsed && (
                        <>
                          <span className="truncate">{label}</span>
                          {count !== undefined && (
                            <span className={`ml-auto text-[10.5px] font-medium ${active ? 'text-brand' : 'text-ink-3'}`}>{count}</span>
                          )}
                        </>
                      )}
                    </button>
                  )
                  return sidebarContentCollapsed ? (
                    <Tooltip key={key}>
                      <TooltipTrigger asChild>{item}</TooltipTrigger>
                      <TooltipContent side="right">{fullLabel}</TooltipContent>
                    </Tooltip>
                  ) : (
                    item
                  )
                })}
              </div>
            ))}
          </TooltipProvider>
        </nav>
        {/* 身份区（设计稿 .side-user）：头像 + 名称 / 角色 + 退出（hover 转 danger） */}
        <div className={`flex items-center border-t border-hairline ${sidebarContentCollapsed ? 'flex-col gap-[9px] py-2.5' : 'gap-[9px] px-3 py-2.5'}`}>
          <div aria-hidden="true" className="grid h-7 w-7 shrink-0 place-items-center rounded-[8px] border border-hairline-2 bg-surface-3 text-[11.5px] font-bold text-ink-2">
            {ADMIN_NAME.charAt(0).toUpperCase()}
          </div>
          {!sidebarContentCollapsed && (
            <div className="min-w-0">
              <div className="text-[12px] font-semibold leading-[1.3]">{ADMIN_NAME}</div>
              <div className="text-[10px] text-ink-3">{t('dashboard.adminRole')}</div>
            </div>
          )}
          <Button
            variant="ghost"
            size="icon"
            className={`h-7 w-7 shrink-0 text-ink-3 hover:bg-danger-soft hover:text-danger ${sidebarContentCollapsed ? '' : 'ml-auto'}`}
            onClick={handleLogout}
            title={t('common.logout')}
            aria-label={t('common.logout')}
          >
            <LogOut className="h-3.5 w-3.5" />
          </Button>
        </div>
        {/* 页脚（设计稿 .side-foot）：运行状态点 + 版本号 + 主题切换 */}
        <div className={`flex items-center border-t border-hairline ${sidebarContentCollapsed ? 'flex-col gap-[9px] py-2.5' : 'gap-2 px-[14px] py-2.5'}`}>
          <span
            role="img"
            aria-label={serverStatusLabel}
            title={serverStatusLabel}
            className={`h-1.5 w-1.5 shrink-0 rounded-full ring-[3px] ${serverHealthy ? 'bg-ok ring-ok-soft' : 'bg-ink-3 ring-surface-3'}`}
          />
          {/* 加载中 / 请求失败时连同版本号一并隐藏，只留灰点，避免展示 `v...` 这类无效版本 */}
          {!sidebarContentCollapsed && serverHealthy && (
            <a
              href="https://github.com/TsinHzl/kiro2cc-proxy"
              target="_blank"
              rel="noopener noreferrer"
              className="truncate text-[10.5px] text-ink-3 hover:text-brand transition-colors"
            >
              kiro2cc-proxy v{serverInfo.version}
            </a>
          )}
          <Button
            variant="ghost"
            size="icon"
            className={`h-7 w-7 shrink-0 text-ink-3 hover:bg-surface-3 hover:text-ink-2 ${sidebarContentCollapsed ? '' : 'ml-auto'}`}
            onClick={(e) => toggleTheme(e.clientX, e.clientY)}
            title={theme === 'dark' ? t('dashboard.toggleLightMode') : t('dashboard.toggleDarkMode')}
            aria-label={theme === 'dark' ? t('dashboard.toggleLightMode') : t('dashboard.toggleDarkMode')}
          >
            {theme === 'dark' ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
          </Button>
        </div>
        </div>
      </aside>
  )
}
