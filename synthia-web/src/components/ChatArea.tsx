interface ChatAreaProps {
  messages: Array<{
    id: string
    role: 'user' | 'assistant' | 'system' | 'tool'
    content: string
    events?: Array<Record<string, unknown>>
  }>
  isLoading: boolean
}

export function ChatArea({ messages, isLoading }: ChatAreaProps) {
  return (
    <div className="chat-area">
      <div className="messages">
        {messages.map(msg => (
          <div key={msg.id} className={`message ${msg.role}`}>
            <div className="message-avatar">
              {msg.role === 'user' ? '👤' : msg.role === 'assistant' ? '🤖' : '⚙️'}
            </div>
            <div className="message-body">
              <div className="message-content">{msg.content}</div>
              {msg.events && msg.events.length > 0 && (
                <div className="message-events">
                  {msg.events.map((ev, i) => (
                    <div key={i} className="event-tag">{String(ev.type || 'event')}</div>
                  ))}
                </div>
              )}
            </div>
          </div>
        ))}
        {isLoading && (
          <div className="message assistant thinking">
            <div className="message-avatar">🤖</div>
            <div className="message-body">
              <div className="typing-indicator">
                <span></span><span></span><span></span>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
