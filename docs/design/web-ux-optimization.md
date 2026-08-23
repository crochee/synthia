# Synthia 前端 UX 优化设计稿

> 状态：v1 设计稿（融合行业最佳实践 + 当前仓库代码事实）
> 适用范围：`/home/crochee/workspace/synthia/synthia-web/`
> 上游约束：`mvp-agent-design.md` v1.3（MVP 范围） + `docs/design/synthia-a2a-ui-fusion-design.md` v2 + `docs/design/goallistpanel-aggregation.md` v2

## 0. 现状盘点（2026-08-15）

**技术栈**
- React 18.3 + Vite 6 + TypeScript 5.6
- Radix UI Themes 3.3 + 自研 design tokens（`styles/tokens.css`）
- React Router 6 + Radix 自带 Toast/Heading/Badge
- A2A SDK v1.0（globalThis 单例 client + 5s tick 驱动重渲染）
- localStorage 持久化（`synthia.messages.{sessionId}` + `synthia.sessions.v1`）
- 自托管 Inter / JetBrains Mono（`/public/fonts/`）
- Playwright e2e（`tests/e2e/ui/`、`tests/e2e/integration/`、`tests/e2e/agent/`）
- 代码总量 ≈ 4,550 行（pages + components + api + hooks + lib）

**关键事实**
- 8 个 page，最重的是 `ChatPage.tsx`（759 行）、`ChatMessageView.tsx`（398 行）、`a2a-stream.ts`（684 行）、`task-to-messages.ts`（707 行）
- 主题：单一 light（`appearance="light"`，accentColor="blue"）
- 字体：Inter Variable + JetBrains Mono，自托管 woff2
- 状态：localStorage 持久化（无 IndexedDB / 无 service worker / 无 CDN 缓存）
- 网络：30s 健康轮询（`useServerHealth`，hidden tab 跳过）
- A2A：单例 client 缓存 + `stream/await` + 5s tick
- ChatPage 输入：textarea + Enter 发送、Shift+Enter 换行、auto-grow **未实现**
- 错误处理：stream error 进 segment 文末、`api/client.ts` 转 `Error`，但**无全局 ErrorBoundary**
- 性能：未做虚拟滚动；3–7 个 goal 不需要，但 messages 列表增长无上限
- A11y：`aria-label` 部分覆盖；focus ring 有；颜色对比基本合规

## 1. 行业基线（参考业内领先产品）

| 维度 | ChatGPT / Claude.ai | Cursor | Notion | Linear | Vercel |
|---|---|---|---|---|---|
| 首屏 TTI | < 1s | < 1s | < 1s | < 1s | < 1s |
| 流式响应延迟 | < 100ms | < 80ms | n/a | n/a | < 100ms |
| Markdown 渲染 | 流式 | 流式 | 即时 | 即时 | 流式 |
| 主题切换 | Light/Dark/System | Light/Dark | Light/Dark/System | Light/Dark | Light/Dark |
| 键盘可操作 | 完整 | 完整 | 完整 | 完整 | 完整 |
| 自动保存/恢复 | 即时 | 即时 | 即时 | 即时 | 即时 |
| 折叠/展开思维 | ✓ | ✓ | n/a | n/a | ✓ |
| Stop/Cancel | ✓ | ✓ | n/a | n/a | ✓ |
| 输入体验 | 自动 grow、命令面板、附件 | 命令面板优先 | 即时工具栏 | 命令面板 | 命令面板 |
| 错误反馈 | 顶部 toast + 内联重试 | 内联重试 | toast + undo | toast | toast |

## 2. UX 优化目标

| 目标 | 度量 |
|---|---|
| **G1 首屏响应** | TTI < 1s（routes 切换 < 200ms） |
| **G2 流式流畅** | 流式输入到首字符渲染 < 100ms；打字不卡顿 |
| **G3 视觉一致** | 全局组件复用同一 design tokens；视觉抖动 0 |
| **G4 操作简洁** | ChatPage 关键操作 ≤ 2 次点击；常用快捷键 ≥ 5 个 |
| **G5 错误恢复** | 任意子组件崩溃可恢复；stream 中断有提示 + 一键重连 |

## 3. 设计原则

1. **协议优先不破**：UI 不发明 wire 字段；样式不破坏 wire shape 检测。
2. **最小代码原则**（CLAUDE.md §2）：只改必要代码；不引入新依赖、不重写可工作模块。
3. **Radix 优先**：复用 Radix Themes 已有组件（`Box` / `Flex` / `Text` / `Button` / `Heading` / `Badge`），不引入 shadcn。
4. **设计 token 收敛**：颜色 / 间距 / 圆角 / 阴影 / 字体全部走 `var(--token)`，无内联 hex。
5. **零状态意识**：每个交互组件必须明确 idle / loading / error / disabled 四态。
6. **A11y 第一公民**：所有按钮/输入/icon button 都有 `aria-label`；focus ring 统一。

## 4. 实施路线图（按 ROI 排序）

### 阶段 0：错误兜底 + 全局一致（1 天，**最高 ROI**）

**问题**：当前无 ErrorBoundary，子组件崩溃 = 白屏；Header 在 light theme 下 blue accent 与 background 几乎不区分。

**改动**：

| 改动 | 文件 |
|---|---|
| 新增 `components/layout/ErrorBoundary.tsx`（class 组件） | 新增 |
| 在 `App.tsx` 包裹 `<ErrorBoundary><BrowserRouter /></ErrorBoundary>` | 改 |
| 新增 `components/ui/Toast.tsx` + 简单全局 toast store | 新增 |
| `Header.tsx` badge 改用 Radix `Avatar` / `Status`-style dot，颜色强化 | 改 |

**验收**：手工抛错 → ErrorBoundary 兜底 → 一键 reload；`useServerHealth` 切换 online/offline 时 toast 通知。

### 阶段 1：性能与交互流畅（2 天）

**问题**：ChatPage 流式期间每段事件都触发整树重渲染；localStorage 序列化整 array；messages 列表无虚拟化（长会话会卡）。

**改动**：

| 改动 | 文件 |
|---|---|
| ChatPage：消息持久化 debounce 300ms（避免每个 segment 写盘） | 改 `ChatPage.tsx` |
| ChatPage：extract reducer 用 `useReducer` 替代多个独立 `useState` | 改 `ChatPage.tsx` |
| `ChatMessageView`：单条 message 用 `React.memo`；segment 按 id memoize | 改 `ChatMessageView.tsx` |
| `ChatPage.tsx`：textarea 加 auto-grow（监听内容高度，max 240px） | 改 `ChatPage.tsx` + `ChatPage.css` |
| 引入 `react-window` 做 messages 虚拟滚动（仅当 messages > 50 时启用） | 新增依赖 |
| `a2a-stream.ts`：`extractFromMessage` 用 `useMemo` 在调用处缓存（实际上搬去 page 层更合适） | 改 |

**验收**：300 段长会话流式渲染保持 60fps；打字时无卡顿；快速切会话不闪屏。

### 阶段 2：视觉一致性 + 设计 token 收敛（1 天）

**问题**：`page.css` 大量 hardcode 色（`#115e58`、各种 `rgba`）、inline style 散落各组件；与 v2 融合稿 §4.5 目标（design token 收敛）有差距。

**改动**：

| 改动 | 文件 |
|---|---|
| `tokens.css`：补 `--color-tool-call-bg`、`--color-tool-result-bg`、`--color-thinking-fg`、`--color-thinking-bg` 等 chat 专用 token | 改 |
| `ChatPage.css`：所有 hardcode 色替换为 `var(--token)`；与 tokens.css 一致 | 改 |
| `MainLayout.tsx`、`Header.tsx`、`Sidebar.tsx`：内联 style 改用 className + token | 改 |
| `Header.tsx`：徽章样式统一到 `tokens.css` 的 `--badge-*` | 改 |

**验收**：grep `#[0-9a-f]{3,8}` 全仓 ≤ 20 处（仅 tokens.css）；dark mode 切换零成本。

### 阶段 3：键盘可操作 + A11y（1 天）

**问题**：Sidebar 标注了快捷键（`[C]`、`[T]`），但**实际没绑定**；textarea 仅 Enter 发送，缺 Ctrl+Enter、Cmd+Enter 兜底、Esc 取消；icon button 无 aria-label。

**改动**：

| 改动 | 文件 |
|---|---|
| 全局快捷键 `useKeyboardShortcuts` hook：g+c → /chat、g+t → /tools … | 新增 `hooks/useKeyboardShortcuts.ts` |
| ChatPage：textarea 支持 Enter / Shift+Enter / Cmd+Enter 全部已支持（仅文档化）；新增 Esc 清空（双 Esc 退出） | 改 `ChatPage.tsx` |
| Header / Sidebar 全部 icon button 加 `aria-label` | 改 |
| Sidebar：nav item 改用 `<Button asChild><NavLink/></Button>` | 改 `Sidebar.tsx` |
| ChatMessageList：`role="log"`、`aria-live="polite"`、`aria-label="Conversation"` | 改 |

**验收**：axe-core e2e 跑过；Lighthouse Accessibility ≥ 95。

### 阶段 4：错误处理 + 流中断恢复（1 天）

**问题**：`useServerHealth` 仅顶栏徽章；stream 中断（fetch 异常）当前只在 ChatPage 末尾追加 error 文本，**用户难以发现**。

**改动**：

| 改动 | 文件 |
|---|---|
| `a2a-stream.ts::sendMessageStream`：把 AbortError / NetworkError 分类为 `error.type: 'network'`、`'aborted'`、`'server'` | 改 |
| ChatPage：在消息流顶部展示 banner 错误（红条 + Retry 按钮），不淹没后续流 | 改 |
| stream retry：自动 retry-once + 退避（500ms → 1.5s）；超过一次按钮化 retry | 改 |
| `useServerHealth` 离线时禁用 ChatPage 提交按钮（disabled + tooltip） | 改 |
| 全局 toast：成功 / 失败 / 信息三档；4s 自动消失；可手动 dismiss | 新增 `hooks/useToast.ts` |

**验收**：手动关 server → ChatPage 输入禁用 + 红条；恢复后 toast 通知；stream 中断 1 次自动重连。

### 阶段 5：主题切换（可选 1 天，仅当 P0/P1 完成后有空）

**变更**：从 light-only 扩到 dark mode。`tokens.css` 加 `:root[data-theme='dark']` 覆盖；`App.tsx` 读取 `localStorage.theme` 默认值；Header 加 ThemeToggle。

> 此项**不强制**，待前 4 阶段稳定后再评估。

## 5. 关键文件改动清单（落地范围）

| 文件 | 改动类型 | 阶段 |
|---|---|---|
| `src/App.tsx` | 加 ErrorBoundary + ToastProvider | 0 |
| `src/main.tsx` | 加主题初始化（inline script） | 5 |
| `src/components/layout/ErrorBoundary.tsx` | 新增 | 0 |
| `src/components/layout/Header.tsx` | 重构样式（去 inline），加 Toast 接入 | 0, 2 |
| `src/components/layout/Sidebar.tsx` | 样式收敛 + navLink 重构 | 2, 3 |
| `src/components/layout/MainLayout.tsx` | 样式收敛 | 2 |
| `src/components/ui/Toast.tsx` | 新增 | 0 |
| `src/components/ui/Button.tsx` | 加 loading / disabled-as-tooltip | 4 |
| `src/pages/ChatPage.tsx` | useReducer + debounce + auto-grow + error banner + retry | 1, 4 |
| `src/pages/ChatPage.css` | 颜色 token 化 + auto-grow | 1, 2 |
| `src/components/chat/ChatMessageView.tsx` | memo + aria 属性 + ToolBlock 渲染优化 | 1, 3 |
| `src/api/chat-stream.ts` | 错误分类 + retry-once | 4 |
| `src/api/client.ts` | 加 AbortSignal 透传 + 错误标准化 | 4 |
| `src/hooks/useServerHealth.ts` | 暴露事件总线（让 ChatPage 订阅 offline） | 4 |
| `src/hooks/useKeyboardShortcuts.ts` | 新增 | 3 |
| `src/hooks/useToast.ts` | 新增 | 0 |
| `src/styles/tokens.css` | 补 chat / badge / status token | 0, 2, 5 |
| `src/styles/page.css` | 颜色 token 化 | 2 |

## 6. 关键体验优化项（用户视角）

### 6.1 ChatPage 输入体验

| 当前 | 优化 |
|---|---|
| textarea 固定 3 行 | auto-grow 1–10 行（min 60px，max 240px） |
| Enter 发，Shift+Enter 换行 | 维持；补充 Cmd/Ctrl+Enter 也发送（Power user） |
| 输入时 disabled | 维持；改进为"提交后立刻可输入下一条，与流并行"（业界一致做法） |
| Esc 无效果 | Esc 清空草稿；二连 Esc 取消进行中流（V2） |
| 草稿持久化无 | localStorage `synthia.draft.{sessionId}`，切会话不丢 |

### 6.2 ChatPage 流式体验

| 当前 | 优化 |
|---|---|
| 用户消息不可重新生成 | hover 显示 "regenerate"（V2） |
| 助手消息可复制 | 维持；加 "copy" 按钮（hover 显示） |
| Thinking 折叠 | 维持；加"按住展开"预览 |
| 长消息无锚点 | 暂不做（V2） |
| 中断/取消无 | Stage 4 加 Stop 按钮（cancel 进行中流） |

### 6.3 Header / Sidebar

| 当前 | 优化 |
|---|---|
| Header 仅 logo + 状态徽章 | 加版本号、ThemeToggle（V2）、命令面板入口 |
| Sidebar 220px 固定 | 维持；P2 不引入可拖拽 |
| Sidebar 快捷键标注但不工作 | Stage 3 全局快捷键实现 |
| 无面包屑 | 加面包屑（chat / chat/:id / tasks / tasks/:id） |
| 无 favicon / title 动态化 | 加动态 title（"Synthia · Session ABC…"） |

### 6.4 列表页（Tools / Skills / Agents / Tasks）

| 当前 | 优化 |
|---|---|
| 卡片网格无搜索 | 加搜索框（已部分有：Tasks 有 memory_search） |
| 无排序 | 加列点击排序（id / name / updated_at） |
| 无批量选择 | 暂不做（V2） |
| 加载时空白 | 加 skeleton loading（Radix `Skeleton`） |
| 空状态文字 | 加 empty state illustration + CTA |

### 6.5 错误处理

| 当前 | 优化 |
|---|---|
| 崩溃白屏 | ErrorBoundary + 一键 reload |
| stream error 进文本末尾 | 顶部 banner + 重试按钮 |
| API 错误 toast 无 | 全局 toast 接入 |
| 健康轮询 30s 太慢 | 5s（首次）→ 30s（稳定后） |

## 7. 不做项（明确划出）

- 不引入 shadcn/ui / MUI / Antd 等其他组件库
- 不引入状态管理库（Redux / Zustand）；React Hooks + Context 已够
- 不做服务端渲染 / Next.js
- 不做多主题切换（Stage 5 可选；本稿未列必做）
- 不做完整 i18n（V2）
- 不做 agent URL 发现 / Trace 可视化（V2）
- 不做 virtual list 复杂动效（仅阶段 1 基础虚拟滚动）

## 8. 验收标准

### 性能
- [ ] Lighthouse Performance ≥ 90
- [ ] Lighthouse Accessibility ≥ 95
- [ ] Lighthouse Best Practices ≥ 90
- [ ] 首屏 TTI < 1s
- [ ] 流式输入延迟 < 100ms

### 功能
- [ ] ErrorBoundary 覆盖所有 page
- [ ] Toast 系统接入 Header / ChatPage
- [ ] ChatPage textarea auto-grow + 草稿持久化
- [ ] ChatPage 流中断 banner + retry
- [ ] 全局快捷键 g+c/t/g/k/a 工作
- [ ] Sidebar 标注与实现一致

### 质量
- [ ] 前端 0 lint（`npm run lint`）
- [ ] TypeScript 0 error（`npm run typecheck`）
- [ ] Playwright e2e 通过（`npm run test:ui` + `test:integration`）
- [ ] 0 hardcode 颜色（除 `tokens.css`）

### 视觉
- [ ] Radix 组件统一
- [ ] 设计 token 全覆盖
- [ ] hover / focus / disabled / loading 四态齐全

## 9. 风险与权衡

| 风险 | 缓解 |
|---|---|
| ErrorBoundary 引入过深 | 仅包 `<BrowserRouter>` 外层；page 内局部错误单独 try/catch |
| debounce 持久化导致刷新丢消息 | 关键事件（用户提交、stream end）立刻同步写盘 |
| react-window 引入增加 bundle | 仅 messages > 50 条件加载；MVP 不引入 |
| useReducer 重构 ChatPage 引入回归 | 现有 11 个 useState 各自独立语义清晰；先做 debounce + auto-grow，useReducer 留给 Stage 1 后段 |
| 全局快捷键与浏览器冲突 | 仅 `g+<letter>` 双击前缀；不抢占单键 |
| ThemeToggle 提前实现偏离 scope | Stage 5 可选；不在前 4 阶段必做 |

## 10. 与上游设计稿的衔接

| 关联 | 处理 |
|---|---|
| `synthia-a2a-ui-fusion-design.md` v2 §4 MVP 必需项 | 本稿阶段 0–4 全包含（M2 GoalListPanel 单独走 goallistpanel-aggregation.md） |
| `mvp-agent-design.md` §11.2 安全/权限砍头 | 本稿 0 引入 |
| `mvp-agent-design.md` §11.1 不做项 | 全部遵守（无完整主题切换、无 shadcn、无 Zustand） |
| `goallistpanel-aggregation.md` v2 §3.4 wire shape | ChatPage 流式处理已支持；本稿不破坏 |

## 11. 实施批次（实际可执行）

按 ROI 与依赖关系定 5 批，每批独立可回滚：

1. **批 1（错误兜底）**：ErrorBoundary + Toast
2. **批 2（设计 token 收敛）**：tokens.css 补 chat token；page.css / inline style token 化
3. **批 3（ChatPage 输入与流式优化）**：auto-grow + 草稿持久化 + 5s tick 优化
4. **批 4（错误恢复 + 主题扩展基础）**：stream 错误分类 + banner + retry + server health 联动
5. **批 5（键盘与 A11y）**：快捷键 + aria-label 全面补齐

> **不引入**：react-window（V2）/ react-query（V2）/ framer-motion（V2）。