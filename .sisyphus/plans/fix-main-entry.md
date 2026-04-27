# Fix: Missing "main" field in electron-app package.json

## Context

electron-vite builds the main process to `dist/main/main.js` but `package.json` lacks a `"main"` field, causing the error:
```
Error: No entry point found for electron app, please add a "main" field to package.json
```

## Work Objectives

### Must Have
- [ ] Add `"main": "dist/main/main.js"` to `packages/electron-app/package.json`

### Must NOT Have
- No other changes needed

## TODOs

- [ ] 1. Add "main" field to package.json

  **What to do**:
  - Add `"main": "dist/main/main.js",` to `packages/electron-app/package.json` before the `"scripts"` field

  **References**:
  - `packages/electron-app/electron.vite.config.ts:42` — confirms output is `dist/main`
  - `packages/electron-app/package.json` — file to modify

  **Acceptance Criteria**:
  - [ ] `"main"` field present in package.json with value `"dist/main/main.js"`

  **QA Scenarios**:

  ```
  Scenario: Verify fix - run dev server
    Tool: Bash (npm run dev)
    Preconditions: Node modules installed
    Steps:
      1. cd packages/electron-app && npm run dev
      2. Observe electron app launches without "No entry point" error
    Expected Result: Dev server starts, Electron window opens
    Failure Indicators: Error "No entry point found" reappears
    Evidence: Terminal output showing Electron starting
  ```

## Success Criteria

- `npm run dev` in `packages/electron-app` starts without the "No entry point" error
