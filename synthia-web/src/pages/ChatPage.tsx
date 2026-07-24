import { useState, useRef, useEffect, type FormEvent, type KeyboardEvent } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Button } from '../components/ui/Button';
import { Card } from '../components/ui/Card';
import { taskStateToJSON } from '@a2a-js/sdk';
import { sendMessageStream, type A2AStreamEvent } from '../api/a2a-stream';
import './ChatPage.css';

/**
 * Map A2A TaskState enum names (TASK_STATE_*) to the CSS class
 * suffix used by nt-chat__message-status (.status-{suffix}).
 * Accepts both raw enum names (TASK_STATE_COMPLETED) and the
 * unprefixed lowercase form (completed) for resilience.
 */
function normalizeTaskState(state: string): string {
  const stripped = state.replace(/^TASK_STATE_/, '').toLowerCase();
  return stripped || 'unknown';
}

interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  taskId?: string;
  status?: string;
}

const STORAGE_KEY = 'synthia.sessions.v1';

interface SessionMeta {
  id: string;
  title: string;
  createdAt: string;
}

/**
 * Main chat page. Sends user messages to the A2A backend via
 * `message/stream` and renders incremental assistant text as
 * SSE events arrive. Session id is read from the route param
 * and persisted to localStorage.
 */
export function ChatPage() {
  const { sessionId: routeSessionId } = useParams<{ sessionId?: string }>();
  const navigate = useNavigate();

  // Ensure a session exists: if none in URL, create one and
  // redirect to /chat/:sessionId so the URL is shareable.
  useEffect(() => {
    if (!routeSessionId) {
      const id = crypto.randomUUID();
      navigate(`/chat/${id}`, { replace: true });
    }
  }, [routeSessionId, navigate]);

  const sessionId = routeSessionId;

  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Persist session metadata
  useEffect(() => {
    if (!sessionId) return;
    const raw = localStorage.getItem(STORAGE_KEY);
    const sessions: SessionMeta[] = raw ? JSON.parse(raw) : [];
    if (!sessions.find((s) => s.id === sessionId)) {
      const meta: SessionMeta = {
        id: sessionId,
        title: `Session ${sessions.length + 1}`,
        createdAt: new Date().toISOString(),
      };
      sessions.push(meta);
      localStorage.setItem(STORAGE_KEY, JSON.stringify(sessions));
    }
  }, [sessionId]);

  // Restore messages for the current session
  useEffect(() => {
    if (!sessionId) return;
    const raw = localStorage.getItem(`synthia.messages.${sessionId}`);
    setMessages(raw ? JSON.parse(raw) : []);
  }, [sessionId]);

  // Persist messages whenever they change
  useEffect(() => {
    if (!sessionId) return;
    localStorage.setItem(`synthia.messages.${sessionId}`, JSON.stringify(messages));
  }, [sessionId, messages]);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSubmit = async (e?: FormEvent) => {
    e?.preventDefault();
    const text = input.trim();
    if (!text || isStreaming || !sessionId) return;

    const userMessage: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      content: text,
    };
    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setIsStreaming(true);

    const assistantId = crypto.randomUUID();
    setMessages((prev) => [
      ...prev,
      { id: assistantId, role: 'assistant', content: '', status: 'working' },
    ]);

    try {
      for await (const event of sendMessageStream(text, sessionId)) {
        applyStreamEvent(assistantId, event);
      }
    } catch (err) {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId
            ? {
                ...m,
                content: m.content + `\n\n[error: ${(err as Error).message}]`,
                status: 'failed',
              }
            : m,
        ),
      );
    } finally {
      setIsStreaming(false);
    }
  };

  const applyStreamEvent = (assistantId: string, event: A2AStreamEvent) => {
    if (event.type === 'error') {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId
            ? {
                ...m,
                content: m.content + `\n[error ${event.error!.code}: ${event.error!.message}]`,
                status: 'failed',
              }
            : m,
        ),
      );
      return;
    }

    switch (event.type) {
      case 'statusUpdate': {
        if (!event.statusUpdate) return;
        const raw = event.statusUpdate.status.state;
        const state = normalizeTaskState(taskStateToJSON(raw));
        // Extract text from status message if present
        const statusMsg = event.statusUpdate.status.message;
        const inline = (statusMsg?.parts ?? [])
          .map((p) => {
            // SDK v0.3.x uses kind: 'text' for text parts
            if ((p as any).kind === 'text') {
              return (p as any).text ?? '';
            }
            return '';
          })
          .join('');
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId
              ? {
                  ...m,
                  status: state,
                  content: inline && inline !== m.content ? m.content + inline : m.content,
                }
              : m,
          ),
        );
        break;
      }

      case 'message': {
        if (!event.message) return;
        const text = (event.message.parts ?? [])
          .map((p) => {
            if ((p as any).kind === 'text') {
              return (p as any).text ?? '';
            }
            return '';
          })
          .join('');
        if (text) {
          setMessages((prev) =>
            prev.map((m) => (m.id === assistantId ? { ...m, content: m.content + text } : m)),
          );
        }
        break;
      }

      case 'artifactUpdate': {
        if (!event.artifactUpdate) return;
        const text = (event.artifactUpdate.artifact.parts ?? [])
          .map((p) => {
            if ((p as any).kind === 'text') {
              return (p as any).text ?? '';
            }
            return '';
          })
          .join('');
        if (text) {
          setMessages((prev) =>
            prev.map((m) => (m.id === assistantId ? { ...m, content: m.content + text } : m)),
          );
        }
        break;
      }

      case 'task': {
        if (!event.task) return;
        const taskState = event.task.status?.state;
        const state = taskState ? normalizeTaskState(taskStateToJSON(taskState)) : 'unknown';
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? { ...m, status: state } : m)),
        );
        break;
      }
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <div className="nt-chat">
      <div className="nt-chat__messages" data-testid="chat-messages">
        {messages.length === 0 && (
          <Card title="System" glow="green">
            <p>
              Welcome to <strong>Synthia</strong>. Type a message below to start an A2A task.
              Session: <code>{sessionId?.slice(0, 8)}</code>
            </p>
          </Card>
        )}
        {messages.map((msg) => (
          <div
            key={msg.id}
            className={`nt-chat__message nt-chat__message--${msg.role}`}
            data-role={msg.role}
            data-testid={`message-${msg.role}`}
          >
            <div className="nt-chat__message-meta">
              <span className="nt-chat__message-role">
                {msg.role === 'user' ? '> USER' : '> ASSISTANT'}
              </span>
              {msg.status && (
                <span className={`nt-chat__message-status status-${msg.status}`}>{msg.status}</span>
              )}
            </div>
            <div className="nt-chat__message-content">{msg.content}</div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      <form onSubmit={handleSubmit} className="nt-chat__form">
        <textarea
          className="nt-chat__input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
          rows={3}
          disabled={isStreaming}
          data-testid="chat-input"
          aria-label="Message input"
        />
        <Button type="submit" disabled={!input.trim() || isStreaming} data-testid="send-button">
          {isStreaming ? 'Streaming...' : 'Send'}
        </Button>
      </form>
    </div>
  );
}
