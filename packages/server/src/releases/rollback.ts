/**
 * rollback.ts - Shared rollback state machine types and helpers.
 *
 * This module defines the update-meta.json schema and helper functions.
 * Used by both Electron and Tauri rollback implementations.
 */

export type UpdateState = "idle" | "backed_up" | "installing" | "started" | "confirmed" | "failed" | "rolled_back"

export interface UpdateMeta {
  state: UpdateState
  /** The version that was running before the update (the rollback target) */
  oldVersion: string
  /** The version that was just installed */
  newVersion: string
  /** File system path to the backup of the old version */
  backupPath: string
  /** ISO timestamp when this meta was created */
  createdAt: string
}

export interface RollbackStatus {
  needsRollback: boolean
  oldVersion: string
  newVersion: string
  /** True if this is a clean first start after update (health window) */
  cleanStart?: boolean
  /** Human-readable message describing the status */
  message?: string
}

/** Create a fresh UpdateMeta for a backup operation */
export function createBackupMeta(oldVersion: string, backupPath: string): UpdateMeta {
  return {
    state: "backed_up",
    oldVersion,
    newVersion: "",
    backupPath,
    createdAt: new Date().toISOString(),
  }
}

/** Transition: update download is complete and about to be installed */
export function markInstalling(meta: UpdateMeta, newVersion: string): UpdateMeta {
  return { ...meta, state: "installing", newVersion }
}

/** Transition: app started successfully after update */
export function markStarted(meta: UpdateMeta): UpdateMeta {
  return { ...meta, state: "started" }
}

/** Transition: update confirmed (health window passed or clean exit) */
export function markConfirmed(meta: UpdateMeta): UpdateMeta {
  return { ...meta, state: "confirmed" }
}

/** Transition: rollback performed */
export function markRolledBack(meta: UpdateMeta): UpdateMeta {
  return { ...meta, state: "rolled_back" }
}

/** Check if the state indicates a rollback is needed */
export function needsRollback(meta: UpdateMeta): boolean {
  return meta.state === "started" || meta.state === "failed"
}

/** Check if update is in progress (should not be interrupted) */
export function isUpdateInProgress(meta: UpdateMeta): boolean {
  return ["backed_up", "installing", "started"].includes(meta.state)
}

/** Check if this is a fresh install that needs the health window */
export function isNewInstall(meta: UpdateMeta): boolean {
  return meta.state === "installing"
}

/** Default path components for the update meta directory */
export function getUpdaterDirName(): string {
  return "embeddedcowork-updater"
}

export function getMetaFileName(): string {
  return "update-meta.json"
}

export function getBackupDirName(): string {
  return "backup"
}
