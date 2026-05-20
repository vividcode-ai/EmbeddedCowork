import type { Component } from "solid-js"
import AgentSelector from "./agent-selector"
import ModelSelector from "./model-selector"
import ThinkingSelector from "./thinking-selector"
import WorktreeSelector from "./worktree-selector"

interface PromptInputBottomBarProps {
  instanceId: string
  sessionId: string
  currentAgent: string
  currentModel: { providerId: string; modelId: string }
  onAgentChange: (agent: string) => Promise<void>
  onModelChange: (model: { providerId: string; modelId: string }) => Promise<void>
}

const PromptInputBottomBar: Component<PromptInputBottomBarProps> = (props) => {
  return (
    <div class="prompt-input-bottom-bar">
      <div class="prompt-input-bottom-bar__start">
        <AgentSelector
          instanceId={props.instanceId}
          sessionId={props.sessionId}
          currentAgent={props.currentAgent}
          onAgentChange={props.onAgentChange}
        />
        <ModelSelector
          instanceId={props.instanceId}
          sessionId={props.sessionId}
          currentModel={props.currentModel}
          onModelChange={props.onModelChange}
        />
        <ThinkingSelector
          instanceId={props.instanceId}
          currentModel={props.currentModel}
        />
      </div>
      <div class="prompt-input-bottom-bar__end">
        <WorktreeSelector
          instanceId={props.instanceId}
          sessionId={props.sessionId}
        />
      </div>
    </div>
  )
}

export default PromptInputBottomBar
