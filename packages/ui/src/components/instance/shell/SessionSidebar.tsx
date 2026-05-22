import { Show, type Accessor, type Component } from "solid-js"
import type { SessionThread } from "../../../stores/session-state"
import type { Session } from "../../../types/session"
import type { KeyboardShortcut } from "../../../lib/keyboard-registry"
import type { DrawerViewState } from "./types"

import { PlusSquare, Search } from "lucide-solid"
import IconButton from "@suid/material/IconButton"
import MenuOpenIcon from "@suid/icons-material/MenuOpen"
import PushPinIcon from "@suid/icons-material/PushPin"
import PushPinOutlinedIcon from "@suid/icons-material/PushPinOutlined"

import SessionList from "../../session-list"
import KeyboardHint from "../../keyboard-hint"
import { useScrollbarFade } from "../../../lib/hooks/use-scrollbar-fade"
import { getLogger } from "../../../lib/logger"

const log = getLogger("session")

interface SessionSidebarProps {
  t: (key: string) => string
  instanceId: string
  threads: Accessor<SessionThread[]>
  activeSessionId: Accessor<string | null>
  activeSession: Accessor<Session | null>

  showSearch: Accessor<boolean>
  onToggleSearch: () => void

  keyboardShortcuts: Accessor<KeyboardShortcut[]>
  isPhoneLayout: Accessor<boolean>
  drawerState: Accessor<DrawerViewState>
  leftPinned: Accessor<boolean>

  onSelectSession: (sessionId: string) => void
  onNewSession: () => Promise<void> | void
  onPinLeftDrawer: () => void
  onUnpinLeftDrawer: () => void
  onCloseLeftDrawer: () => void

  setContentEl: (el: HTMLElement | null) => void
}

const SessionSidebar: Component<SessionSidebarProps> = (props) => {
  let rootEl: HTMLDivElement | undefined
  const { isHovered, handleMouseEnter, handleMouseLeave } = useScrollbarFade(() => rootEl)

  return (
    <div
      class="flex flex-col h-full min-h-0"
      classList={{ "session-sidebar--scroll-hover": isHovered() }}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      ref={(el) => { rootEl = el; props.setContentEl(el) }}
    >
      <div class="flex flex-col gap-2 px-4 py-3 border-b border-base">
        <div class="flex items-center justify-between gap-2">
          <span class="session-sidebar-title text-sm font-semibold uppercase text-primary">
            {props.t("instanceShell.leftPanel.sessionsTitle")}
          </span>
          <div class="flex items-center gap-2 text-primary">
            <IconButton
              size="small"
              color="inherit"
              aria-label={props.t("sessionList.actions.newSession.ariaLabel")}
              title={props.t("sessionList.actions.newSession.title")}
              onClick={() => {
                const result = props.onNewSession()
                if (result instanceof Promise) {
                  void result.catch((error) => log.error("Failed to create session:", error))
                }
              }}
            >
              <PlusSquare class="w-5 h-5" />
            </IconButton>
            <IconButton
              size="small"
              color="inherit"
              aria-label={props.t("sessionList.filter.ariaLabel")}
              title={props.t("sessionList.filter.ariaLabel")}
              aria-pressed={props.showSearch()}
              onClick={props.onToggleSearch}
              sx={{
                color: props.showSearch() ? "var(--text-primary)" : "inherit",
                backgroundColor: props.showSearch() ? "var(--surface-hover)" : "transparent",
                "&:hover": {
                  backgroundColor: "var(--surface-hover)",
                },
              }}
            >
              <Search class="w-5 h-5" />
            </IconButton>
            <Show when={!props.isPhoneLayout()}>
              <IconButton
                size="small"
                color="inherit"
                aria-label={props.leftPinned() ? props.t("instanceShell.leftDrawer.unpin") : props.t("instanceShell.leftDrawer.pin")}
                onClick={() => (props.leftPinned() ? props.onUnpinLeftDrawer() : props.onPinLeftDrawer())}
              >
                {props.leftPinned() ? <PushPinIcon fontSize="small" /> : <PushPinOutlinedIcon fontSize="small" />}
              </IconButton>
            </Show>
            <Show when={props.drawerState() === "floating-open"}>
              <IconButton
                size="small"
                color="inherit"
                aria-label={props.t("instanceShell.leftDrawer.toggle.close")}
                title={props.t("instanceShell.leftDrawer.toggle.close")}
                onClick={props.onCloseLeftDrawer}
              >
                <MenuOpenIcon fontSize="small" />
              </IconButton>
            </Show>
          </div>
        </div>
        <div class="session-sidebar-shortcuts">
          <Show when={props.keyboardShortcuts().length}>
            <KeyboardHint shortcuts={props.keyboardShortcuts()} separator=" " showDescription={false} />
          </Show>
        </div>
      </div>

      <div class="session-sidebar flex flex-col flex-1 min-h-0">
        <SessionList
          instanceId={props.instanceId}
          threads={props.threads()}
          activeSessionId={props.activeSessionId()}
          onSelect={props.onSelectSession}
          onNew={() => {
            const result = props.onNewSession()
            if (result instanceof Promise) {
              void result.catch((error) => log.error("Failed to create session:", error))
            }
          }}
          enableFilterBar={props.showSearch()}
          showHeader={false}
          showFooter={false}
        />
      </div>
    </div>
  )
}

export default SessionSidebar
