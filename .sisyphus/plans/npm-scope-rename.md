# 重命名计划: @neuralnomads → @vividcode

## TL;DR

> 将项目 npm scope 从 `@neuralnomads` 重命名为 `@vividcode`
> 
> **涉及的包**:
> - `@neuralnomads/embedcowork` → `@vividcode/embedcowork`
> - `@neuralnomads/embedcowork-electron-app` → `@vividcode/embedcowork-electron-app`
> 
> **Estimated Effort**: Short
> **Wave**: 1 (并行执行)

---

## Context

用户请求将 npm scope 从 neuralnomads 重命名为 vividcode。

## 需要修改的文件

| 文件 | 修改内容 | 行数 |
|------|----------|------|
| `packages/server/package.json` | name: `@vividcode/embedcowork` | 2 |
| `packages/electron-app/package.json` | name, dependencies, appId | 3 处 |
| `package.json` (根目录) | workspace scripts | 5 处 |
| `packages/server/README.md` | npx 命令示例 | 多处 |
| `README.md` | npx 命令示例 | 多处 |

## TODOs

- [x] 1. 修改 packages/server/package.json

  **What to do**:
  - `"@neuralnomads/embedcowork"` → `"@vividcode/embedcowork"` (name 字段)
  - `"neuralnomads.ai"` → `"vividcode.ai"` (email)

  **Acceptance Criteria**:
  - [x] 文件中无 @neuralnomads 字符串
  - [x] name 字段为 @vividcode/embedcowork

- [x] 2. 修改 packages/electron-app/package.json

  **What to do**:
  - name: `@neuralnomads/embedcowork-electron-app` → `@vividcode/embedcowork-electron-app`
  - dependencies: `@neuralnomads/embedcowork` → `@vividcode/embedcowork`
  - appId: `ai.neuralnomads.embedcowork.client` → `ai.vividcode.embedcowork.client`

  **Acceptance Criteria**:
  - [x] 文件中无 @neuralnomads 字符串
  - [x] appId 包含 vividcode

- [x] 3. 修改根目录 package.json

  **What to do**:
  - 所有 workspace 脚本引用: `@neuralnomads/` → `@vividcode/`

  **Acceptance Criteria**:
  - [x] 无 @neuralnomads 引用

- [x] 4. 修改 packages/server/README.md

  **What to do**:
  - npx 命令示例: `@neuralnomads` → `@vividcode`
  - 文档链接: NeuralNomadsAI → VividCodeAI

  **Acceptance Criteria**:
  - [x] 无 @neuralnomads 引用

- [x] 5. 修改根目录 README.md

  **What to do**:
  - npx 命令示例: `@neuralnomads` → `@vividcode`
  - 文档链接: NeuralNomadsAI → VividCodeAI

  **Acceptance Criteria**:
  - [x] 无 @neuralnomads 引用

---

## Success Criteria

- [x] 所有 package.json 中无 @neuralnomads 引用
- [x] 所有文档中无 @neuralnomads 引用
- [x] npm install 可以成功执行