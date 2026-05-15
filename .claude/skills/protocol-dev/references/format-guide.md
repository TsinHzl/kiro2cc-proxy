# 回复格式规范

## 禁止使用 Markdown 表格

由于对话框不支持表格渲染，所有需要对比或列举的信息，请使用以下替代格式：

- **列表形式**：用无序列表或有序列表展示
- **分组描述**：用加粗标题 + 缩进描述的方式
- **对比格式**：使用 `A vs B` 或分段描述的方式

## 功能对比示例（禁止用表格）

**Axum Handler**
- 路由注册：✅ 类型安全的 extractor
- 错误处理：✅ 自定义 IntoResponse
- 中间件：✅ Tower 生态

**Actix-web Handler**
- 路由注册：✅ 宏标注
- 错误处理：✅ ResponseError trait
- 中间件：⚠️ 自有生态

## 文件修改清单格式

需要修改的文件：
- `src/admin/handlers.rs`：添加新的 API handler
- `src/admin/router.rs`：注册新路由
- `src/admin/types.rs`：添加请求/响应类型定义

## 方案输出规范

在提出技术方案时，文件清单必须遵循以下格式：

### 新增文件

必须标注完整的文件路径（从项目根目录开始）

示例：

- `src/admin/middleware.rs`：认证中间件
- `src/kiro/retry.rs`：重试策略模块
- `admin-ui/src/components/new-component.tsx`：新 UI 组件

### 修改文件

同样标注完整路径

示例：

- `src/admin/router.rs`：注册新路由
- `Cargo.toml`：添加新依赖

### 执行后提醒

代码修改完成后，必须提醒用户：

1. 执行 `cargo check` 进行语法检查
2. 执行 `cargo build` 进行完整编译
3. 如涉及前端修改，在 `admin-ui/` 下执行 `npm run build`
