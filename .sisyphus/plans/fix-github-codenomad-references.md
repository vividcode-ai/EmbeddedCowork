# .github 目录 codenomad 名称修复计划

## 摘要

修复 .github 目录中所有使用 codenomad/CodeNomad 名称的文件，使其与当前项目 EmbeddedCowork 保持一致。

## 问题分析

当前项目信息：
- **项目名称**: EmbeddedCowork
- **npm 包名**: `@vividcode/embedcowork-*` (如 `@vividcode/embedcowork-electron-app`)
- **GitHub 组织**: VividCodeAI
- **App 名称**: EmbeddedCowork

## 需要修复的文件和具体问题

### 1. bug_report.yml
| 行号 | 当前值 | 应改为 |
|------|--------|--------|
| 2 | `CodeNomad` | `EmbeddedCowork` |

### 2. build-and-upload.yml
| 行号 | 当前值 | 应改为 |
|------|--------|--------|
| 102 | `@neuralnomads/codenomad-electron-app` | `@vividcode/embedcowork-electron-app` |
| 113, 154 | `CodeNomad.app` | `EmbeddedCowork.app` |
| 165 | `CodeNomad-${VERSION_TO_USE}-mac-${arch}.zip` | `EmbeddedCowork-${VERSION_TO_USE}-mac-${arch}.zip` |
| 180 | `CodeNomad-*-mac-*.zip` | `EmbeddedCowork-*-mac-*.zip` |
| 260 | `@neuralnomads/codenomad-electron-app` | `@vividcode/embedcowork-electron-app` |
| 309 | `@neuralnomads/codenomad-electron-app` | `@vividcode/embedcowork-electron-app` |
| 365, 449 | `@codenomad/tauri-app` | `@embedcowork/tauri-app` |
| 393, 477 | `CodeNomad.app` | `EmbeddedCowork.app` |
| 394, 478 | `CodeNomad-Tauri-${VERSION}-macos-x64.zip` | `EmbeddedCowork-Tauri-${VERSION}-macos-x64.zip` |
| 534, 636 | `@codenomad/tauri-app` | `@embedcowork/tauri-app` |
| 566 | `CodeNomad-Tauri-$env:VERSION-windows-x64.zip` | `EmbeddedCowork-Tauri-$env:VERSION-windows-x64.zip` |
| 680-682 | `CodeNomad-Tauri-${VERSION}-linux-x64.*` | `EmbeddedCowork-Tauri-${VERSION}-linux-x64.*` |
| 772 | `@codenomad/tauri-app` | `@embedcowork/tauri-app` |
| 783 | `codenomad-tauri` | `embedcowork-tauri` |
| 785-787 | `CodeNomad-Tauri-${VERSION}-linux-x64.zip` | `EmbeddedCowork-Tauri-${VERSION}-linux-x64.zip` |
| 840 | `@neuralnomads/codenomad-electron-app` | `@vividcode/embedcowork-electron-app` |

### 3. comment-pr-artifacts.yml
| 行号 | 当前值 | 应改为 |
|------|--------|--------|
| 58 | `codenomad-pr-artifacts` | `embedcowork-pr-artifacts` |

### 4. dev-release.yml
| 行号 | 当前值 | 应改为 |
|------|--------|--------|
| 76 | `@neuralnomads/codenomad-dev` | `@vividcode/embedcowork-dev` |

### 5. manual-npm-publish.yml
| 行号 | 当前值 | 应改为 |
|------|--------|--------|
| 16 | `@neuralnomads/codenomad-dev` | `@vividcode/embedcowork-dev` |
| 18 | `@neuralnomads/codenomad` | `@vividcode/embedcowork` |
| 36 | `@neuralnomads/codenomad` | `@vividcode/embedcowork` |

### 6. release-ui.yml
| 行号 | 当前值 | 应改为 |
|------|--------|--------|
| 47 | `@codenomad/ui` | `@embedcowork/ui` |
| 54 | `CODENOMAD_R2_BUCKET` | `EMBEDCOWORK_R2_BUCKET` |

### 7. release.yml
| 行号 | 当前值 | 应改为 |
|------|--------|--------|
| 17 | `@neuralnomads/codenomad` | `@vividcode/embedcowork` |

## 修复模式总结

1. **App 名称**: `CodeNomad` → `EmbeddedCowork`
2. **npm 包名**: 
   - `@neuralnomads/codenomad-*` → `@vividcode/embedcowork-*`
   - `@codenomad/*` → `@embedcowork/*`
3. **变量名**: `CODENOMAD_*` → `EMBEDCOWORK_*`
4. **注释标记**: `codenomad-*` → `embedcowork-*`

## 任务列表

- [x] 1. 修复 bug_report.yml
- [x] 2. 修复 build-and-upload.yml (多处引用)
- [x] 3. 修复 comment-pr-artifacts.yml
- [x] 4. 修复 dev-release.yml
- [x] 5. 修复 manual-npm-publish.yml
- [x] 6. 修复 release-ui.yml
- [x] 7. 修复 release.yml

## 验证方法

修复后检查：
1. 确认没有遗留的 `codenomad` 或 `CodeNomad` 引用
2. 确认所有 npm 包名都使用 `@vividcode/embedcowork-*` 或 `@embedcowork/*` 格式
3. 确认所有 App 名称都是 `EmbeddedCowork`