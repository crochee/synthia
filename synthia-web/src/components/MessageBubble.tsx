interface MessageBubbleProps {
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  events?: Array<Record<string, unknown>>
}

export function MessageBubble({ role, content, events }: MessageBubbleProps) {
  return (
    <div className={`message-bubble ${role}`}>
      {content && <div className="message-content">{content}</div>}
      {events && events.length > 0 && (
        <div className="message-events">
          {events.map((ev, i) => (
            <div key={i} className="event-tag">{String(ev.type || 'event')}</div>
          ))}
        </div>
      )}
    </div>
  )
}
