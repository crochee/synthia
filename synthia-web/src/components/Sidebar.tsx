import { useState, useEffect } from 'react'
import { api } from '../api/client'
import type { Session } from '../api/types'

interface SidebarProps {
  currentSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onCreateSession: () => void
  onDeleteSession: (sessionId: string) => void
}

export function Sidebar({ currentSessionId, onSelectSession, onCreateSession, onDeleteSession }: SidebarProps) {
  const [sessions, setSessions] = useState<Session[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    loadSessions()
  }, [])

  const loadSessions = async () => {
    try {
      const data = await api.get<Session[]>('/api/sessions')
      setSessions(data)
    } catch (err) {
      console.error('Failed to load sessions:', err)
    } finally {
      setLoading(false)
    }
  }

  const handleDelete = async (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await api.delete(`/api/sessions/${sessionId}`)
      setSessions(prev => prev.filter(s => s.session_id !== sessionId))
      onDeleteSession(sessionId)
    } catch (err) {
      console.error('Failed to delete session:', err)
    }
  }

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h2>Sessions</h2>
        <button onClick={onCreateSession} className="btn btn-create">+ New</button>
      </div>
      <div className="session-list">
        {loading ? (
          <div className="loading">Loading...</div>
        ) : sessions.length === 0 ? (
          <div className="empty-state">No sessions yet</div>
        ) : (
          sessions.map(session => (
            <div
              key={session.session_id}
              className={`session-item ${currentSessionId === session.session_id ? 'active' : ''}`}
              onClick={() => onSelectSession(session.session_id)}
            >
              <div className="session-name">
                {session.session_id.slice(0, 16)}...
              </div>
              <div className="session-meta">
                <span className={`status-dot ${session.state}`}></span>
                {session.state}
              </div>
              <button
                className="btn-delete"
                onClick={(e) => handleDelete(session.session_id, e)}
              >
                ×
              </button>
            </div>
          ))
        )}
      </div>
    </aside>
  )
}
