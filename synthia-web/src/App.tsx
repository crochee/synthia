import { useState, useRef, useEffect, useCallback } from 'react'
import './App.css'
import { ChatArea } from './components/ChatArea'
import { MessageBubble } from './components/MessageBubble'
import { ToolCallDisplay } from './components/ToolCallDisplay'
import { Sidebar } from './components/Sidebar'
import { ConnectionStatus } from './components/ConnectionStatus'
import { useWebSocket } from './hooks/useWebSocket'
import { api } from './api/client'

interface AgentEvent {
  type: string
  [key: string]: unknown
}

interface Message {
  id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  events?: AgentEvent[]
  timestamp: Date
  attachments?: File[]
}

const WS_URL = import.meta.env.VITE_WS_URL || 'ws://localhost:3000'

function App() {
  const [messages, setMessages] = useState<Message[]>([])
  const [input, setInput] = useState('')
  const [attachedFiles, setAttachedFiles] = useState<File[]>([])
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [sidebarOpen, setSidebarOpen] = useState(false)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const eventBufferRef = useRef<AgentEvent[]>([])

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages])

  const handleWsMessage = useCallback((data: unknown) => {
    try {
      const agentEvent: AgentEvent = typeof data === 'string' ? JSON.parse(data) : data as AgentEvent
      eventBufferRef.current.push(agentEvent)

      setMessages(prev => {
        const lastMsg = prev[prev.length - 1]
        if (lastMsg?.role === 'assistant' && lastMsg.events) {
          return [...prev.slice(0, -1), { ...lastMsg, events: [...lastMsg.events, agentEvent] }]
        }
        const assistantMsg: Message = {
          id: `assistant-${Date.now()}`,
          role: 'assistant',
          content: '',
          events: [agentEvent],
          timestamp: new Date(),
        }
        return [...prev, assistantMsg]
      })

      if (agentEvent.type === 'SessionEnded') {
        setIsLoading(false)
      }
    } catch {
      console.error('Failed to parse event:', data)
    }
  }, [])

  const { sendMessage, isConnected } = useWebSocket({
    url: WS_URL,
    onMessage: handleWsMessage,
  })

  const createSession = async () => {
    try {
      const res = await api.post<{ session_id: string }>('/api/sessions')
      setSessionId(res.session_id)
      setMessages([])
      setSidebarOpen(false)
    } catch (err) {
      console.error('Failed to create session:', err)
      const newSessionId = `session-${Date.now()}`
      setSessionId(newSessionId)
      setMessages([])
    }
  }

  const handleDeleteSession = (deletedId: string) => {
    if (sessionId === deletedId) {
      setSessionId(null)
      setMessages([])
    }
  }

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files) {
      setAttachedFiles(prev => [...prev, ...Array.from(e.target.files!)])
    }
  }

  const removeFile = (index: number) => {
    setAttachedFiles(prev => prev.filter((_, i) => i !== index))
  }

  const sendMessageHandler = () => {
    if ((!input.trim() && attachedFiles.length === 0) || !isConnected || isLoading) return

    const userMsg: Message = {
      id: `user-${Date.now()}`,
      role: 'user',
      content: input.trim(),
      timestamp: new Date(),
      attachments: attachedFiles.length > 0 ? [...attachedFiles] : undefined,
    }
    setMessages(prev => [...prev, userMsg])

    const payload = JSON.stringify({
      session_id: sessionId,
      input: input.trim(),
      files: attachedFiles.map(f => ({ name: f.name, type: f.type })),
    })

    sendMessage(payload)
    setInput('')
    setAttachedFiles([])
    setIsLoading(true)
    eventBufferRef.current = []
  }

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      sendMessageHandler()
    }
  }

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault()
    if (e.dataTransfer.files) {
      setAttachedFiles(prev => [...prev, ...Array.from(e.dataTransfer.files)])
    }
  }

  const renderFilePreview = (file: File) => {
    if (file.type.startsWith('image/')) {
      return <img src={URL.createObjectURL(file)} alt={file.name} className="file-preview" />
    }
    return <div className="file-preview text">📄 {file.name}</div>
  }

  return (
    <div className="app">
      {sidebarOpen && (
        <div className="sidebar-overlay" onClick={() => setSidebarOpen(false)} />
      )}
      <Sidebar
        currentSessionId={sessionId}
        onSelectSession={(id) => { setSessionId(id); setSidebarOpen(false) }}
        onCreateSession={createSession}
        onDeleteSession={handleDeleteSession}
      />

      <main className="main-content">
        <header className="header">
          <button className="mobile-menu-btn" onClick={() => setSidebarOpen(!sidebarOpen)}>
            ☰
          </button>
          <h1>Synthia AI Agent</h1>
          <div className="header-controls">
            <ConnectionStatus wsUrl={WS_URL} />
            {sessionId && <span className="session-id">Session: {sessionId.slice(0, 12)}...</span>}
            <button onClick={createSession} className="btn btn-secondary">New Session</button>
          </div>
        </header>

        <ChatArea messages={messages} isLoading={isLoading} />

        <div className="input-area" onDrop={handleDrop} onDragOver={e => e.preventDefault()}>
          {attachedFiles.length > 0 && (
            <div className="attached-files">
              {attachedFiles.map((file, i) => (
                <div key={i} className="attached-file">
                  {renderFilePreview(file)}
                  <button onClick={() => removeFile(i)} className="remove-file">×</button>
                </div>
              ))}
            </div>
          )}
          <div className="input-row">
            <input
              ref={fileInputRef}
              type="file"
              multiple
              onChange={handleFileSelect}
              style={{ display: 'none' }}
              accept="image/*,.txt,.pdf,.csv,.json"
            />
            <button onClick={() => fileInputRef.current?.click()} className="btn btn-attach">📎</button>
            <textarea
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder="Type a message..."
              rows={2}
              disabled={!isConnected || isLoading}
            />
            <button onClick={sendMessageHandler} className="btn btn-send" disabled={!isConnected || isLoading}>
              {isLoading ? '⏳' : '➤'}
            </button>
          </div>
        </div>
      </main>
    </div>
  )
}

export default App
