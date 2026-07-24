interface ToolCallDisplayProps {
  toolName: string
  status: 'pending' | 'running' | 'completed' | 'error'
  output?: string
}

export function ToolCallDisplay({ toolName, status, output }: ToolCallDisplayProps) {
  const statusIcon = {
    pending: '⏳',
    running: '🔄',
    completed: '✅',
    error: '❌',
  }[status]

  return (
    <div className={`tool-call ${status}`}>
      <div className="tool-call-header">
        <span className="tool-call-icon">{statusIcon}</span>
        <span className="tool-call-name">{toolName}</span>
        <span className={`tool-call-status status-${status}`}>{status}</span>
      </div>
      {output && <div className="tool-call-output">{output}</div>}
    </div>
  )
}
