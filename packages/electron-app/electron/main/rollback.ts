import { app } from "electron"
import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync, cpSync } from "fs"
import { join } from "path"

export interface UpdateMeta {
  state: "idle" | "backed_up" | "installing" | "started" | "confirmed" | "failed"
  oldVersion: string
  newVersion: string
  backupPath: string
  createdAt: string
}

export interface RollbackStatus {
  needsRollback: boolean
  oldVersion: string
  newVersion: string
  cleanStart?: boolean
}

const UPDATER_DIR = "embeddedcowork-updater"
const META_FILE = "update-meta.json"
const BACKUP_DIR = "backup"

export class ElectronRollbackManager {
  private updaterDir: string
  private metaPath: string
  private backupRoot: string

  constructor() {
    const userDataPath = app.getPath("userData")
    this.updaterDir = join(userDataPath, UPDATER_DIR)
    this.metaPath = join(this.updaterDir, META_FILE)
    this.backupRoot = join(this.updaterDir, BACKUP_DIR)
  }

  private ensureDir(dir: string) {
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true })
    }
  }

  private readMeta(): UpdateMeta | null {
    try {
      if (!existsSync(this.metaPath)) return null
      return JSON.parse(readFileSync(this.metaPath, "utf-8")) as UpdateMeta
    } catch {
      return null
    }
  }

  private writeMeta(meta: UpdateMeta) {
    this.ensureDir(this.updaterDir)
    writeFileSync(this.metaPath, JSON.stringify(meta, null, 2), "utf-8")
  }

  /** Call BEFORE updating: backup current app installation */
  async backupCurrentApp(version: string): Promise<string> {
    const backupPath = join(this.backupRoot, `v${version}`)
    this.ensureDir(backupPath)

    const appPath = app.getAppPath()
    if (existsSync(appPath)) {
      cpSync(appPath, join(backupPath, "app"), { recursive: true, force: true })
    }

    const exePath = process.execPath
    if (existsSync(exePath)) {
      cpSync(exePath, join(backupPath, "executable"), { recursive: true, force: true })
    }

    this.writeMeta({
      state: "backed_up",
      oldVersion: version,
      newVersion: "",
      backupPath,
      createdAt: new Date().toISOString(),
    })

    return backupPath
  }

  /** Mark that an update is about to be applied */
  markInstalling(newVersion: string) {
    const meta = this.readMeta() || {
      state: "backed_up",
      oldVersion: "",
      newVersion: "",
      backupPath: "",
      createdAt: new Date().toISOString(),
    }
    meta.state = "installing"
    meta.newVersion = newVersion
    this.writeMeta(meta)
  }

  /** Check on startup if rollback is needed */
  async checkStartup(): Promise<RollbackStatus | null> {
    const meta = this.readMeta()
    if (!meta) return null

    if (meta.state === "installing") {
      // Previous update installed but app may have crashed
      // Mark as started, then if next startup still has "installing", we know crash happened
      const oldMeta = { ...meta }
      meta.state = "started"
      this.writeMeta(meta)
      return {
        needsRollback: false,
        oldVersion: oldMeta.oldVersion,
        newVersion: oldMeta.newVersion,
        cleanStart: true,
      }
    }

    if (meta.state === "started") {
      // App was started but not confirmed - this means previous session crashed
      // because confirmed was never written, but we detected it on a subsequent launch
      return {
        needsRollback: true,
        oldVersion: meta.oldVersion,
        newVersion: meta.newVersion,
      }
    }

    return null
  }

  /** Mark that update was successful after healthy startup window */
  async confirmHealthyStart(): Promise<void> {
    const meta = this.readMeta()
    if (!meta) return
    meta.state = "confirmed"
    this.writeMeta(meta)
    // Clean up backup
    this.cleanBackup()
  }

  /** Mark clean exit on app quit */
  async markCleanExit(): Promise<void> {
    const meta = this.readMeta()
    if (!meta) return
    // If we were in "started" state and app exits normally, mark confirmed
    if (meta.state === "started") {
      meta.state = "confirmed"
      this.writeMeta(meta)
      this.cleanBackup()
    }
  }

  /** Execute rollback: restore from backup and relaunch */
  async rollbackToPrevious(): Promise<void> {
    const meta = this.readMeta()
    if (!meta || !existsSync(meta.backupPath)) {
      throw new Error("No backup available for rollback")
    }

    // Restore backup - electron-updater will install the previous version package
    // For NSIS: we use the backup's installer
    const backupAppDir = join(meta.backupPath, "app")
    const currentAppPath = app.getAppPath()

    if (existsSync(backupAppDir) && existsSync(currentAppPath)) {
      // Remove current and restore backup
      rmSync(currentAppPath, { recursive: true, force: true })
      cpSync(backupAppDir, currentAppPath, { recursive: true, force: true })
    }

    meta.state = "rolled_back"
    this.writeMeta(meta)
    this.cleanBackup()
  }

  /** Clean up backup directory */
  cleanBackup() {
    const meta = this.readMeta()
    if (meta?.backupPath && existsSync(meta.backupPath)) {
      rmSync(meta.backupPath, { recursive: true, force: true })
    }
  }
}
