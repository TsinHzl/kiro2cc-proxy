# 变更提案：redesign-kam-import-dialog

## 背景
依据高保真原型 `/Users/MacBook/Downloads/kam-import-modal-redesign/index.html`，将 admin-ui 的 KAM 批量导入弹窗（`web-ui/admin-ui/src/components/batch-import-dialog.tsx`）的 UI 与交互升级为新设计：现有弹窗仅为纯 textarea + 提示文案，缺少拖拽导入、实时 JSON 校验反馈、快捷工具栏与文件选择，录入体验差且语法错误只能在提交后暴露。

## 目标范围
**在范围内：**
- 弹窗头部：主标题 + 胶囊徽章「支持单账号 / 批量导入」
- JSON 输入区上方快捷工具栏：填入示例 / 格式化 / 清空
- Dropzone：整块输入区支持拖拽 .json 文件填入，拖拽悬停高亮覆盖层；点击选择本地文件（限 .json、2MB）
- 实时 JSON 语法健康度状态条：输入防抖解析，🟢 格式正确·已识别 X 个账号 / 🔴 语法错误（含错误信息）
- 琥珀色卡片式验活规则提示（替换现有 💡 纯文本 hint）
- 底部按钮组保留：KAM下载 / 取消 / 导入，导入按钮加载态改用 SVG spinner（Loader2）
- i18n 文案同步（zh.json / en.json 新增 key）
- 完整保留现有导入验活业务流：解析 → sha256 查重 → addCredential(disabled) → getCredentialBalance 验活 → 启用，逐账号状态机 + 进度条 + 结果列表 + Toast 一律不动

**不在范围内：**
- 后端 Rust 代码、API 接口
- 导入验活业务逻辑、结果状态机的任何行为变更
- user-ui
- 新增 npm 依赖（拖拽/文件读取用原生 DragEvent + FileReader）

## 技术方案
- 仅改造 `batch-import-dialog.tsx` 单组件：新增 `dragActive`、`jsonStatus`（idle/valid/invalid + accountCount/error）状态；useEffect 防抖 300ms 实时 JSON.parse 并按现有解析规则统计账号数（array / accounts 字段 / 单对象）
- 示例文案复用现有 `batchImportPlaceholder`（即完整 KAM 导出示例）作为「填入示例」内容
- 文件校验：后缀 .json、大小 ≤ 2MB，超限 toast.error
- 品牌色沿用现有 token（text-brand / bg-brand 等），不硬编码色值
- Dialog 打开时重置新增状态，导入过程中禁用工具栏/拖拽/文件选择

## 预期影响
- 仅前端 admin-ui 单组件 + i18n 文件，无 API/协议变更
- 现有导入流程行为不变，纯录入侧体验增强
- 构建验证：`cd web-ui/admin-ui && pnpm install && pnpm build`

## 风险
- 大文件粘贴拖拽解析卡顿 → 防抖 + 2MB 上限缓解
- 拖拽事件在 Dialog 覆盖层下的事件冒泡 → 在输入区容器上 preventDefault 处理 dragover/dragleave
- i18n key 遗漏导致英文环境缺翻译 → zh/en 同步新增并自检
