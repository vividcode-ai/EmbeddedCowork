import { Show, createEffect, createSignal } from "solid-js"
import type { ServerMeta } from "../../../server/src/api-types"
import { getServerMeta } from "../lib/server-meta"
import { useI18n } from "../lib/i18n"

export default function VersionPill() {
  const { t } = useI18n()
  const [meta, setMeta] = createSignal<ServerMeta | null>(null)

  createEffect(() => {
    void getServerMeta()
      .then((result) => setMeta(result))
      .catch(() => setMeta(null))
  })

  const serverVersion = () => meta()?.serverVersion
  const uiVersion = () => meta()?.ui?.version
  const uiSource = () => meta()?.ui?.source
  const opencodeVersion = () => meta()?.opencodeVersion

  const uiLabel = () => (uiVersion() ? t("versionPill.uiWithVersion", { version: uiVersion() }) : t("versionPill.ui"))
  const showSepBeforeAgent = () => serverVersion() || uiVersion() || uiSource()

  return (
    <Show when={serverVersion() || uiVersion() || uiSource() || opencodeVersion()}>
      <div class="text-[11px] text-muted whitespace-nowrap">
        <Show when={serverVersion()}>
          {(v) => <span>{t("versionPill.appWithVersion", { version: v() })}</span>}
        </Show>
        <Show when={uiVersion() || uiSource()}>
          <>
            <Show when={serverVersion()}>
              <span class="mx-2">·</span>
            </Show>
            <span>
              {uiLabel()}
              <Show when={uiSource()}>{(s) => <span class="opacity-70">{t("versionPill.source", { source: s() })}</span>}</Show>
            </span>
          </>
        </Show>
        <Show when={opencodeVersion()}>
          {(v) => (
            <>
              <Show when={showSepBeforeAgent()}>
                <span class="mx-2">·</span>
              </Show>
              <span>{t("versionPill.agent", { version: v() })}</span>
            </>
          )}
        </Show>
      </div>
    </Show>
  )
}
