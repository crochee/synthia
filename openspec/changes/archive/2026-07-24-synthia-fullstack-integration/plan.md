# Synthia Full-Stack Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete full-stack integration for Synthia with A2A protocol frontend, Neon Terminal design, 8 feature pages, Makefile toolchain, and three-layer E2E testing.

**Architecture:** Frontend (React + Vite + @a2a-js/sdk) communicates with backend (Rust + Axum) via A2A protocol. Frontend served by Nginx reverse proxy. Parallel development across frontend, backend, and engineering tracks.

**Tech Stack:** React 18, TypeScript, Vite 6, @a2a-js/sdk, React Router v6, Playwright, Rust/Axum, tower-http CORS, Make, Docker, Nginx

---

## Task 1: Backend CORS Configuration (Task 1.1)

**Files:**
- Modify: `crates/synthia-server/src/config/server.rs`
- Modify: `crates/synthia-server/src/server/router.rs`

- [ ] **Step 1: Add CorsConfig struct to server config**

In `crates/synthia-server/src/config/server.rs`, add:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorsConfig {
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
}

fn default_allowed_origins() -> Vec<String> {
    vec!["http://localhost:5173".to_string()]
}

fn default_allowed_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string(), "PUT".to_string(), "DELETE".to_string(), "OPTIONS".to_string()]
}

fn default_allowed_headers() -> Vec<String> {
    vec!["Content-Type".to_string(), "Authorization".to_string()]
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_allowed_origins(),
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
        }
    }
}
```

Add `cors` field to `ServerConfig`:

```rust
#[serde(default)]
pub cors: CorsConfig,
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p synthia-server`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-server/src/config/server.rs
git commit -m "feat(server): add CorsConfig struct to server configuration"
```

---

## Task 2: Backend CORS Layer Integration (Task 1.2)

**Files:**
- Modify: `crates/synthia-server/src/server/router.rs`
- Modify: `crates/synthia-server/Cargo.toml`

- [ ] **Step 1: Add tower-http dependency**

In `crates/synthia-server/Cargo.toml`, add to `[dependencies]`:

```toml
tower-http = { version = "0.5", features = ["cors"] }
```

- [ ] **Step 2: Add CORS layer to router**

In `crates/synthia-server/src/server/router.rs`, add CORS layer creation:

```rust
use tower_http::cors::{CorsLayer, Any};

fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    let origins: Vec<axum::http::HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
}
```

Apply the layer in router construction:

```rust
let cors = build_cors_layer(&state.config.cors);
// Add .layer(cors) to the router
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p synthia-server`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-server/Cargo.toml crates/synthia-server/src/server/router.rs
git commit -m "feat(server): integrate tower-http CORS layer with configurable origins"
```

---

## Task 3: Frontend A2A SDK Installation (Task 1.3)

**Files:**
- Modify: `synthia-web/package.json`

- [ ] **Step 1: Install @a2a-js/sdk**

Run: `cd synthia-web && npm install @a2a-js/sdk`

- [ ] **Step 2: Verify installation**

Run: `cd synthia-web && npm ls @a2a-js/sdk`
Expected: Shows installed version

- [ ] **Step 3: Commit**

```bash
git add synthia-web/package.json synthia-web/package-lock.json
git commit -m "feat(web): install @a2a-js/sdk for A2A protocol support"
```

---

## Task 4: Frontend A2A Client Module (Task 1.4)

**Files:**
- Create: `synthia-web/src/api/a2a-client.ts`

- [ ] **Step 1: Create A2A client wrapper**

Create `synthia-web/src/api/a2a-client.ts`:

```typescript
import { A2AClient } from '@a2a-js/sdk';

const A2A_BASE_URL = import.meta.env.VITE_A2A_URL || 'http://localhost:8080';

export const a2aClient = new A2AClient(A2A_BASE_URL);
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add synthia-web/src/api/a2a-client.ts
git commit -m "feat(web): create A2A client wrapper module"
```

---

## Task 5: Frontend A2A sendMessage (Task 1.5)

**Files:**
- Create: `synthia-web/src/api/a2a-send.ts`

- [ ] **Step 1: Implement sendMessage function**

Create `synthia-web/src/api/a2a-send.ts`:

```typescript
import { a2aClient } from './a2a-client';

export async function sendMessage(text: string, sessionId?: string) {
  const message = {
    role: 'user' as const,
    parts: [{ kind: 'text' as const, text }],
    messageId: crypto.randomUUID(),
  };

  const params: any = {
    jsonrpc: '2.0',
    method: 'message/send',
    params: {
      taskId: sessionId || crypto.randomUUID(),
      message,
    },
  };

  return a2aClient.request(params);
}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add synthia-web/src/api/a2a-send.ts
git commit -m "feat(web): implement A2A sendMessage function"
```

---

## Task 6: Frontend A2A Streaming (Task 1.6)

**Files:**
- Create: `synthia-web/src/api/a2a-stream.ts`

- [ ] **Step 1: Implement sendMessageStream function**

Create `synthia-web/src/api/a2a-stream.ts`:

```typescript
import { a2aClient } from './a2a-client';

export async function* sendMessageStream(text: string, sessionId?: string) {
  const message = {
    role: 'user' as const,
    parts: [{ kind: 'text' as const, text }],
    messageId: crypto.randomUUID(),
  };

  const params: any = {
    jsonrpc: '2.0',
    method: 'message/stream',
    params: {
      taskId: sessionId || crypto.randomUUID(),
      message,
    },
  };

  const stream = await a2aClient.stream(params);
  for await (const event of stream) {
    yield event;
  }
}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add synthia-web/src/api/a2a-stream.ts
git commit -m "feat(web): implement A2A streaming response handler"
```

---

## Task 7: Root Makefile (Task 1.7)

**Files:**
- Create: `Makefile`

- [ ] **Step 1: Create Makefile with all commands**

Create `Makefile` in project root:

```makefile
.PHONY: help dev build test deploy docker clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

dev: ## Start frontend and backend dev servers
	@echo "Starting backend on :8080 and frontend on :5173..."
	@make -j2 dev-server dev-web

dev-server: ## Start backend dev server
	cd crates/synthia-server && cargo watch -x run

dev-web: ## Start frontend dev server
	cd synthia-web && npm run dev

build: build-web build-server ## Build both frontend and backend

build-web: ## Build frontend for production
	cd synthia-web && npm run build

build-server: ## Build backend for production
	cargo build --release -p synthia-server

test: test-rust test-web ## Run all tests

test-rust: ## Run Rust tests
	cargo test --workspace

test-web: ## Run frontend tests
	cd synthia-web && npm test

test-e2e: ## Run E2E tests
	cd synthia-web && npx playwright test

deploy: build ## Deploy (build all)
	@echo "Deployment artifacts ready"

docker: ## Build Docker images
	docker build -f Dockerfile.server -t synthia-server .
	docker build -f Dockerfile.web -t synthia-web .

docker-up: ## Start Docker Compose
	docker-compose up -d

docker-down: ## Stop Docker Compose
	docker-compose down

clean: ## Clean build artifacts
	cargo clean
	cd synthia-web && rm -rf dist node_modules
```

- [ ] **Step 2: Verify Makefile**

Run: `make help`
Expected: Shows all available commands

- [ ] **Step 3: Commit**

```bash
git add Makefile
git commit -m "feat: add root Makefile for unified build/dev/deploy toolchain"
```

---

## Task 8: Dockerfile.server (Task 1.8)

**Files:**
- Create: `Dockerfile.server`

- [ ] **Step 1: Create multi-stage Dockerfile for backend**

Create `Dockerfile.server`:

```dockerfile
# Build stage
FROM rust:1.95-alpine AS builder
RUN apk add --no-cache musl-dev pkgconfig openssl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p synthia-server

# Runtime stage
FROM alpine:3.19
RUN apk add --no-cache ca-certificates
WORKDIR /app
COPY --from=builder /app/target/release/synthia-server .
COPY config.yaml .
EXPOSE 8080
CMD ["./synthia-server"]
```

- [ ] **Step 2: Verify Dockerfile syntax**

Run: `docker build -f Dockerfile.server --check .` (or `docker build -f Dockerfile.server -t test .` if --check not available)
Expected: No syntax errors

- [ ] **Step 3: Commit**

```bash
git add Dockerfile.server
git commit -m "feat: add Dockerfile.server with Rust 1.95-alpine multi-stage build"
```

---

## Task 9: Dockerfile.web (Task 1.9)

**Files:**
- Create: `Dockerfile.web`

- [ ] **Step 1: Create Dockerfile for frontend**

Create `Dockerfile.web`:

```dockerfile
# Build stage
FROM node:20-alpine AS builder
WORKDIR /app
COPY synthia-web/package*.json ./
RUN npm ci
COPY synthia-web/ .
RUN npm run build

# Runtime stage
FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
```

- [ ] **Step 2: Commit**

```bash
git add Dockerfile.web
git commit -m "feat: add Dockerfile.web with Node 20-alpine build and Nginx runtime"
```

---

## Task 10: Foundation Validation (Task 1.10)

- [ ] **Step 1: Start backend**

Run: `make dev-server`
Expected: Backend starts on :8080

- [ ] **Step 2: Start frontend in another terminal**

Run: `make dev-web`
Expected: Frontend starts on :5173

- [ ] **Step 3: Test A2A message send**

Open browser at `http://localhost:5173`, send a test message.
Expected: Message is sent via A2A protocol and response is received

- [ ] **Step 4: Commit validation**

```bash
git commit --allow-empty -m "chore: validate foundation - frontend and backend connected via A2A"
```

---

## Task 11: Neon Terminal Design Tokens (Task 2.1)

**Files:**
- Create: `synthia-web/src/styles/tokens.css`

- [ ] **Step 1: Create CSS design tokens**

Create `synthia-web/src/styles/tokens.css`:

```css
:root {
  /* Background colors */
  --bg-primary: #0a0a1a;
  --bg-secondary: #0f0f2a;
  --bg-tertiary: #1a1a3a;

  /* Accent colors */
  --accent-green: #00ff88;
  --accent-cyan: #00ddff;
  --accent-purple: #aa55ff;

  /* Text colors */
  --text-primary: #e0e0e0;
  --text-secondary: #888888;
  --text-muted: #555555;

  /* Glow effects */
  --glow-green: 0 0 10px #00ff8866;
  --glow-cyan: 0 0 10px #00ddff66;

  /* Fonts */
  --font-mono: 'JetBrains Mono', 'Courier New', monospace;
  --font-sans: 'Inter', -apple-system, sans-serif;

  /* Spacing */
  --spacing-xs: 4px;
  --spacing-sm: 8px;
  --spacing-md: 16px;
  --spacing-lg: 24px;
  --spacing-xl: 32px;

  /* Border radius */
  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;
}
```

- [ ] **Step 2: Import tokens in main CSS**

In `synthia-web/src/index.css` or `main.tsx`, add:

```css
@import './styles/tokens.css';
```

- [ ] **Step 3: Commit**

```bash
git add synthia-web/src/styles/tokens.css synthia-web/src/index.css
git commit -m "feat(web): add Neon Terminal design tokens"
```

---

## Task 12: Base UI Components (Task 2.2)

**Files:**
- Create: `synthia-web/src/components/ui/Button.tsx`
- Create: `synthia-web/src/components/ui/Input.tsx`
- Create: `synthia-web/src/components/ui/Card.tsx`
- Create: `synthia-web/src/components/ui/Modal.tsx`

- [ ] **Step 1: Create Button component**

Create `synthia-web/src/components/ui/Button.tsx`:

```tsx
import React from 'react';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary';
}

export function Button({ variant = 'primary', className = '', ...props }: ButtonProps) {
  const baseStyle = {
    fontFamily: 'var(--font-mono)',
    padding: 'var(--spacing-sm) var(--spacing-md)',
    borderRadius: 'var(--radius-sm)',
    border: '1px solid',
    cursor: 'pointer',
  };

  const variantStyle = variant === 'primary'
    ? { borderColor: 'var(--accent-green)', color: 'var(--accent-green)', background: 'transparent', boxShadow: 'var(--glow-green)' }
    : { borderColor: 'var(--accent-cyan)', color: 'var(--accent-cyan)', background: 'transparent', boxShadow: 'var(--glow-cyan)' };

  return <button style={{ ...baseStyle, ...variantStyle }} className={className} {...props} />;
}
```

- [ ] **Step 2: Create Input component**

Create `synthia-web/src/components/ui/Input.tsx`:

```tsx
import React from 'react';

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {}

export function Input({ className = '', style, ...props }: InputProps) {
  return (
    <input
      style={{
        fontFamily: 'var(--font-mono)',
        background: 'var(--bg-secondary)',
        border: '1px solid var(--accent-green)',
        color: 'var(--text-primary)',
        padding: 'var(--spacing-sm) var(--spacing-md)',
        borderRadius: 'var(--radius-sm)',
        outline: 'none',
        ...style,
      }}
      className={className}
      {...props}
    />
  );
}
```

- [ ] **Step 3: Create Card component**

Create `synthia-web/src/components/ui/Card.tsx`:

```tsx
import React from 'react';

interface CardProps {
  children: React.ReactNode;
  className?: string;
}

export function Card({ children, className = '' }: CardProps) {
  return (
    <div
      className={className}
      style={{
        background: 'var(--bg-secondary)',
        border: '1px solid var(--bg-tertiary)',
        borderRadius: 'var(--radius-md)',
        padding: 'var(--spacing-lg)',
        boxShadow: 'var(--glow-green)',
      }}
    >
      {children}
    </div>
  );
}
```

- [ ] **Step 4: Create Modal component**

Create `synthia-web/src/components/ui/Modal.tsx`:

```tsx
import React from 'react';

interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: React.ReactNode;
}

export function Modal({ isOpen, onClose, title, children }: ModalProps) {
  if (!isOpen) return null;

  return (
    <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.8)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 }} onClick={onClose}>
      <div style={{ background: 'var(--bg-primary)', border: '1px solid var(--accent-green)', borderRadius: 'var(--radius-md)', padding: 'var(--spacing-xl)', minWidth: '400px', boxShadow: 'var(--glow-green)' }} onClick={e => e.stopPropagation()}>
        <h2 style={{ color: 'var(--accent-green)', fontFamily: 'var(--font-mono)', marginBottom: 'var(--spacing-md)' }}>{title}</h2>
        {children}
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Verify TypeScript compilation**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add synthia-web/src/components/ui/
git commit -m "feat(web): add base UI components (Button, Input, Card, Modal) with Neon Terminal style"
```

---

## Task 13: React Router Setup (Task 2.3)

**Files:**
- Modify: `synthia-web/package.json`
- Create: `synthia-web/src/App.tsx`

- [ ] **Step 1: Install React Router**

Run: `cd synthia-web && npm install react-router-dom`

- [ ] **Step 2: Create App.tsx with routes**

Create `synthia-web/src/App.tsx`:

```tsx
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Navigate to="/chat" replace />} />
        <Route path="/chat" element={<div>Chat Page</div>} />
        <Route path="/chat/:sessionId" element={<div>Chat Session</div>} />
        <Route path="/tools" element={<div>Tools Page</div>} />
        <Route path="/skills" element={<div>Skills Page</div>} />
        <Route path="/settings" element={<div>Settings Page</div>} />
        <Route path="/tasks" element={<div>Tasks Page</div>} />
        <Route path="/memory" element={<div>Memory Page</div>} />
        <Route path="/jobs" element={<div>Jobs Page</div>} />
        <Route path="/mcp" element={<div>MCP Page</div>} />
      </Routes>
    </BrowserRouter>
  );
}
```

- [ ] **Step 3: Update main.tsx to use App**

Modify `synthia-web/src/main.tsx`:

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles/tokens.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

- [ ] **Step 4: Verify**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add synthia-web/package.json synthia-web/package-lock.json synthia-web/src/App.tsx synthia-web/src/main.tsx
git commit -m "feat(web): setup React Router v6 with all page routes"
```

---

## Task 14: Layout Components (Task 2.4)

**Files:**
- Create: `synthia-web/src/components/layout/Header.tsx`
- Create: `synthia-web/src/components/layout/Sidebar.tsx`
- Create: `synthia-web/src/components/layout/MainLayout.tsx`

- [ ] **Step 1: Create Header component**

Create `synthia-web/src/components/layout/Header.tsx`:

```tsx
import React from 'react';

interface HeaderProps {
  isServerAvailable: boolean;
}

export function Header({ isServerAvailable }: HeaderProps) {
  return (
    <header style={{ background: 'var(--bg-secondary)', borderBottom: '1px solid var(--accent-green)', padding: 'var(--spacing-sm) var(--spacing-lg)', display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontFamily: 'var(--font-mono)' }}>
      <h1 style={{ color: 'var(--accent-green)', fontSize: '18px', margin: 0, textShadow: 'var(--glow-green)' }}>SYNTHIA</h1>
      <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--spacing-sm)' }}>
        <div style={{ width: 8, height: 8, borderRadius: '50%', background: isServerAvailable ? 'var(--accent-green)' : '#ff0055', boxShadow: isServerAvailable ? 'var(--glow-green)' : '0 0 10px #ff005566' }} />
        <span style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>{isServerAvailable ? 'CONNECTED' : 'DISCONNECTED'}</span>
      </div>
    </header>
  );
}
```

- [ ] **Step 2: Create Sidebar component**

Create `synthia-web/src/components/layout/Sidebar.tsx`:

```tsx
import React from 'react';
import { NavLink } from 'react-router-dom';

const navItems = [
  { path: '/chat', label: 'CHAT', icon: '💬' },
  { path: '/tools', label: 'TOOLS', icon: '🛠' },
  { path: '/skills', label: 'SKILLS', icon: '📚' },
  { path: '/settings', label: 'SETTINGS', icon: '⚙️' },
  { path: '/tasks', label: 'TASKS', icon: '📊' },
  { path: '/memory', label: 'MEMORY', icon: '🧠' },
  { path: '/jobs', label: 'JOBS', icon: '📅' },
  { path: '/mcp', label: 'MCP', icon: '🔗' },
];

export function Sidebar() {
  return (
    <nav style={{ width: '200px', background: 'var(--bg-secondary)', borderRight: '1px solid var(--bg-tertiary)', padding: 'var(--spacing-md) 0', fontFamily: 'var(--font-mono)' }}>
      {navItems.map(item => (
        <NavLink
          key={item.path}
          to={item.path}
          style={({ isActive }) => ({
            display: 'block',
            padding: 'var(--spacing-sm) var(--spacing-lg)',
            color: isActive ? 'var(--accent-green)' : 'var(--text-secondary)',
            textDecoration: 'none',
            borderLeft: isActive ? '2px solid var(--accent-green)' : '2px solid transparent',
            textShadow: isActive ? 'var(--glow-green)' : 'none',
          })}
        >
          {item.icon} {item.label}
        </NavLink>
      ))}
    </nav>
  );
}
```

- [ ] **Step 3: Create MainLayout component**

Create `synthia-web/src/components/layout/MainLayout.tsx`:

```tsx
import React from 'react';
import { Outlet } from 'react-router-dom';
import { Header } from './Header';
import { Sidebar } from './Sidebar';

interface MainLayoutProps {
  isServerAvailable: boolean;
}

export function MainLayout({ isServerAvailable }: MainLayoutProps) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100vh', background: 'var(--bg-primary)' }}>
      <Header isServerAvailable={isServerAvailable} />
      <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        <Sidebar />
        <main style={{ flex: 1, overflow: 'auto', padding: 'var(--spacing-lg)' }}>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Update App.tsx to use layout**

Update `synthia-web/src/App.tsx` to wrap routes with `MainLayout`:

```tsx
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { MainLayout } from './components/layout/MainLayout';

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<MainLayout isServerAvailable={true} />}>
          <Route path="/" element={<Navigate to="/chat" replace />} />
          <Route path="/chat" element={<div>Chat Page</div>} />
          <Route path="/chat/:sessionId" element={<div>Chat Session</div>} />
          <Route path="/tools" element={<div>Tools Page</div>} />
          <Route path="/skills" element={<div>Skills Page</div>} />
          <Route path="/settings" element={<div>Settings Page</div>} />
          <Route path="/tasks" element={<div>Tasks Page</div>} />
          <Route path="/memory" element={<div>Memory Page</div>} />
          <Route path="/jobs" element={<div>Jobs Page</div>} />
          <Route path="/mcp" element={<div>MCP Page</div>} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
```

- [ ] **Step 5: Verify**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add synthia-web/src/components/layout/ synthia-web/src/App.tsx
git commit -m "feat(web): add layout components (Header, Sidebar, MainLayout) with Neon Terminal style"
```

---

## Task 15: Chat Page (Task 2.5)

**Files:**
- Create: `synthia-web/src/pages/ChatPage.tsx`

- [ ] **Step 1: Create ChatPage component**

Create `synthia-web/src/pages/ChatPage.tsx`:

```tsx
import React, { useState } from 'react';
import { Button } from '../components/ui/Button';
import { Input } from '../components/ui/Input';
import { Card } from '../components/ui/Card';

interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
}

export function ChatPage() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');

  const handleSend = () => {
    if (!input.trim()) return;
    const userMsg: Message = { id: crypto.randomUUID(), role: 'user', content: input };
    setMessages(prev => [...prev, userMsg]);
    setInput('');
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', fontFamily: 'var(--font-mono)' }}>
      <div style={{ flex: 1, overflow: 'auto', padding: 'var(--spacing-md)' }}>
        {messages.map(msg => (
          <Card key={msg.id}>
            <div style={{ color: msg.role === 'user' ? 'var(--accent-green)' : 'var(--accent-cyan)', marginBottom: 'var(--spacing-xs)' }}>
              {msg.role === 'user' ? '> USER' : '> ASSISTANT'}
            </div>
            <div style={{ color: 'var(--text-primary)' }}>{msg.content}</div>
          </Card>
        ))}
      </div>
      <div style={{ display: 'flex', gap: 'var(--spacing-sm)', padding: 'var(--spacing-md)', borderTop: '1px solid var(--bg-tertiary)' }}>
        <Input
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && handleSend()}
          placeholder="Type a message..."
          style={{ flex: 1 }}
        />
        <Button onClick={handleSend}>SEND</Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route**

In `synthia-web/src/App.tsx`, replace `<Route path="/chat" element={<div>Chat Page</div>} />` with:

```tsx
<Route path="/chat" element={<ChatPage />} />
```

Add import: `import { ChatPage } from './pages/ChatPage';`

- [ ] **Step 3: Verify**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add synthia-web/src/pages/ChatPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement ChatPage with message display and input"
```

---

## Task 16: A2A Protocol Integration in Chat (Task 2.6)

**Files:**
- Modify: `synthia-web/src/pages/ChatPage.tsx`

- [ ] **Step 1: Integrate A2A streaming into ChatPage**

Update `synthia-web/src/pages/ChatPage.tsx` to use A2A streaming:

```tsx
import { sendMessageStream } from '../api/a2a-stream';

// In handleSend:
const handleSend = async () => {
  if (!input.trim()) return;
  const userMsg: Message = { id: crypto.randomUUID(), role: 'user', content: input };
  setMessages(prev => [...prev, userMsg]);
  const currentInput = input;
  setInput('');

  // Add placeholder for assistant response
  const assistantMsg: Message = { id: crypto.randomUUID(), role: 'assistant', content: '' };
  setMessages(prev => [...prev, assistantMsg]);

  try {
    for await (const event of sendMessageStream(currentInput)) {
      if (event.result?.artifact?.parts) {
        const text = event.result.artifact.parts
          .filter((p: any) => p.kind === 'text')
          .map((p: any) => p.text)
          .join('');
        setMessages(prev => prev.map(m => m.id === assistantMsg.id ? { ...m, content: m.content + text } : m));
      }
    }
  } catch (err) {
    setMessages(prev => prev.map(m => m.id === assistantMsg.id ? { ...m, content: 'Error: ' + (err as Error).message } : m));
  }
};
```

- [ ] **Step 2: Verify**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add synthia-web/src/pages/ChatPage.tsx
git commit -m "feat(web): integrate A2A streaming protocol into ChatPage"
```

---

## Task 17: Session Management (Task 2.7)

**Files:**
- Create: `synthia-web/src/hooks/useSession.ts`
- Modify: `synthia-web/src/pages/ChatPage.tsx`

- [ ] **Step 1: Create useSession hook**

Create `synthia-web/src/hooks/useSession.ts`:

```typescript
import { useState, useCallback } from 'react';

interface Session {
  id: string;
  name: string;
  createdAt: Date;
}

export function useSession() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);

  const createSession = useCallback((name?: string) => {
    const session: Session = {
      id: crypto.randomUUID(),
      name: name || `Session ${sessions.length + 1}`,
      createdAt: new Date(),
    };
    setSessions(prev => [...prev, session]);
    setCurrentSessionId(session.id);
    return session;
  }, [sessions.length]);

  const deleteSession = useCallback((id: string) => {
    setSessions(prev => prev.filter(s => s.id !== id));
    if (currentSessionId === id) {
      setCurrentSessionId(null);
    }
  }, [currentSessionId]);

  const switchSession = useCallback((id: string) => {
    setCurrentSessionId(id);
  }, []);

  return { sessions, currentSessionId, createSession, deleteSession, switchSession };
}
```

- [ ] **Step 2: Integrate useSession into ChatPage**

In `ChatPage.tsx`, add:

```tsx
import { useSession } from '../hooks/useSession';

// In component:
const { sessions, currentSessionId, createSession, deleteSession, switchSession } = useSession();
```

Add session list UI to ChatPage (sidebar or top bar).

- [ ] **Step 3: Verify**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add synthia-web/src/hooks/useSession.ts synthia-web/src/pages/ChatPage.tsx
git commit -m "feat(web): implement session management (create/list/delete/switch)"
```

---

## Task 18: Connection Status Indicator (Task 2.8)

**Files:**
- Create: `synthia-web/src/hooks/useServerHealth.ts`
- Modify: `synthia-web/src/App.tsx`

- [ ] **Step 1: Create useServerHealth hook**

Create `synthia-web/src/hooks/useServerHealth.ts`:

```typescript
import { useState, useEffect } from 'react';

const HEALTH_URL = '/health';
const CHECK_INTERVAL = 30000;

export function useServerHealth() {
  const [isServerAvailable, setIsServerAvailable] = useState(false);

  useEffect(() => {
    const checkHealth = async () => {
      try {
        const response = await fetch(HEALTH_URL);
        setIsServerAvailable(response.ok);
      } catch {
        setIsServerAvailable(false);
      }
    };

    checkHealth();
    const interval = setInterval(checkHealth, CHECK_INTERVAL);
    return () => clearInterval(interval);
  }, []);

  return isServerAvailable;
}
```

- [ ] **Step 2: Use hook in App.tsx**

In `synthia-web/src/App.tsx`:

```tsx
import { useServerHealth } from './hooks/useServerHealth';

export default function App() {
  const isServerAvailable = useServerHealth();

  return (
    <BrowserRouter>
      <Routes>
        <Route element={<MainLayout isServerAvailable={isServerAvailable} />}>
          {/* ... routes ... */}
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
```

- [ ] **Step 3: Verify**

Run: `cd synthia-web && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add synthia-web/src/hooks/useServerHealth.ts synthia-web/src/App.tsx
git commit -m "feat(web): add server health check hook with 30s polling"
```

---

## Task 19: Phase 2 Validation (Task 2.9)

- [ ] **Step 1: Start dev environment**

Run: `make dev`
Expected: Both servers start

- [ ] **Step 2: Verify chat experience**

Open browser at `http://localhost:5173`. Verify:
- Neon Terminal theme applied
- Chat page works with A2A streaming
- Session management works
- Connection status indicator shows correctly
- Navigation between pages works

- [ ] **Step 3: Commit validation**

```bash
git commit --allow-empty -m "chore: validate Phase 2 - design system and core chat experience complete"
```

---

## Task 20: Tools Page (Task 3.1)

**Files:**
- Create: `synthia-web/src/pages/ToolsPage.tsx`

- [ ] **Step 1: Create ToolsPage**

Create `synthia-web/src/pages/ToolsPage.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import { Card } from '../components/ui/Card';

interface Tool {
  name: string;
  description: string;
  status: string;
}

export function ToolsPage() {
  const [tools, setTools] = useState<Tool[]>([]);

  useEffect(() => {
    fetch('/api/tools')
      .then(r => r.json())
      .then(data => setTools(data || []))
      .catch(() => setTools([]));
  }, []);

  return (
    <div style={{ fontFamily: 'var(--font-mono)' }}>
      <h2 style={{ color: 'var(--accent-green)', textShadow: 'var(--glow-green)' }}>TOOLS</h2>
      {tools.length === 0 ? (
        <Card><div style={{ color: 'var(--text-secondary)' }}>No tools available</div></Card>
      ) : (
        tools.map(tool => (
          <Card key={tool.name}>
            <div style={{ color: 'var(--accent-cyan)' }}>{tool.name}</div>
            <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>{tool.description}</div>
            <div style={{ color: 'var(--accent-green)', fontSize: '11px' }}>{tool.status}</div>
          </Card>
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route**

Replace `<Route path="/tools" element={<div>Tools Page</div>} />` with `<Route path="/tools" element={<ToolsPage />} />`

- [ ] **Step 3: Commit**

```bash
git add synthia-web/src/pages/ToolsPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement ToolsPage"
```

---

## Task 21: Skills Page (Task 3.2)

**Files:**
- Create: `synthia-web/src/pages/SkillsPage.tsx`

- [ ] **Step 1: Create SkillsPage**

Create `synthia-web/src/pages/SkillsPage.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';

interface Skill {
  name: string;
  description: string;
  enabled: boolean;
}

export function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([]);

  useEffect(() => {
    fetch('/api/skills')
      .then(r => r.json())
      .then(data => setSkills(data || []))
      .catch(() => setSkills([]));
  }, []);

  const toggleSkill = async (name: string, enabled: boolean) => {
    await fetch(`/api/skills/${name}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled: !enabled }) });
    setSkills(prev => prev.map(s => s.name === name ? { ...s, enabled: !enabled } : s));
  };

  return (
    <div style={{ fontFamily: 'var(--font-mono)' }}>
      <h2 style={{ color: 'var(--accent-green)', textShadow: 'var(--glow-green)' }}>SKILLS</h2>
      {skills.map(skill => (
        <Card key={skill.name}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <div style={{ color: 'var(--accent-cyan)' }}>{skill.name}</div>
              <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>{skill.description}</div>
            </div>
            <Button variant={skill.enabled ? 'primary' : 'secondary'} onClick={() => toggleSkill(skill.name, skill.enabled)}>
              {skill.enabled ? 'ENABLED' : 'DISABLED'}
            </Button>
          </div>
        </Card>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route and commit**

```bash
git add synthia-web/src/pages/SkillsPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement SkillsPage with toggle functionality"
```

---

## Task 22: Settings Page (Task 3.3)

**Files:**
- Create: `synthia-web/src/pages/SettingsPage.tsx`

- [ ] **Step 1: Create SettingsPage**

Create `synthia-web/src/pages/SettingsPage.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { Input } from '../components/ui/Input';

export function SettingsPage() {
  const [provider, setProvider] = useState('');
  const [model, setModel] = useState('');

  useEffect(() => {
    fetch('/api/settings')
      .then(r => r.json())
      .then(data => { setProvider(data.provider || ''); setModel(data.model || ''); })
      .catch(() => {});
  }, []);

  const save = async () => {
    await fetch('/api/settings', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ provider, model }) });
  };

  return (
    <div style={{ fontFamily: 'var(--font-mono)' }}>
      <h2 style={{ color: 'var(--accent-green)', textShadow: 'var(--glow-green)' }}>SETTINGS</h2>
      <Card>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--spacing-md)' }}>
          <div>
            <label style={{ color: 'var(--text-secondary)', display: 'block', marginBottom: 'var(--spacing-xs)' }}>PROVIDER</label>
            <Input value={provider} onChange={e => setProvider(e.target.value)} style={{ width: '100%' }} />
          </div>
          <div>
            <label style={{ color: 'var(--text-secondary)', display: 'block', marginBottom: 'var(--spacing-xs)' }}>MODEL</label>
            <Input value={model} onChange={e => setModel(e.target.value)} style={{ width: '100%' }} />
          </div>
          <Button onClick={save}>SAVE</Button>
        </div>
      </Card>
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route and commit**

```bash
git add synthia-web/src/pages/SettingsPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement SettingsPage with provider and model configuration"
```

---

## Task 23: Tasks Page (Task 3.4)

**Files:**
- Create: `synthia-web/src/pages/TasksPage.tsx`

- [ ] **Step 1: Create TasksPage**

Create `synthia-web/src/pages/TasksPage.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import { Card } from '../components/ui/Card';

interface Task {
  id: string;
  status: string;
  createdAt: string;
}

export function TasksPage() {
  const [tasks, setTasks] = useState<Task[]>([]);

  useEffect(() => {
    fetch('/api/tasks')
      .then(r => r.json())
      .then(data => setTasks(data || []))
      .catch(() => setTasks([]));
  }, []);

  return (
    <div style={{ fontFamily: 'var(--font-mono)' }}>
      <h2 style={{ color: 'var(--accent-green)', textShadow: 'var(--glow-green)' }}>TASKS</h2>
      {tasks.length === 0 ? (
        <Card><div style={{ color: 'var(--text-secondary)' }}>No tasks</div></Card>
      ) : (
        tasks.map(task => (
          <Card key={task.id}>
            <div style={{ color: 'var(--accent-cyan)' }}>Task: {task.id.slice(0, 8)}</div>
            <div style={{ color: task.status === 'completed' ? 'var(--accent-green)' : 'var(--accent-purple)', fontSize: '12px' }}>{task.status.toUpperCase()}</div>
            <div style={{ color: 'var(--text-muted)', fontSize: '11px' }}>{task.createdAt}</div>
          </Card>
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route and commit**

```bash
git add synthia-web/src/pages/TasksPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement TasksPage with task history"
```

---

## Task 24: Memory Page (Task 3.5)

**Files:**
- Create: `synthia-web/src/pages/MemoryPage.tsx`

- [ ] **Step 1: Create MemoryPage**

Create `synthia-web/src/pages/MemoryPage.tsx`:

```tsx
import React, { useState } from 'react';
import { Card } from '../components/ui/Card';
import { Input } from '../components/ui/Input';
import { Button } from '../components/ui/Button';

interface MemoryEntry {
  id: string;
  content: string;
  relevance: number;
}

export function MemoryPage() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<MemoryEntry[]>([]);

  const search = async () => {
    if (!query.trim()) return;
    try {
      const res = await fetch(`/api/memory/search?q=${encodeURIComponent(query)}`);
      const data = await res.json();
      setResults(data || []);
    } catch {
      setResults([]);
    }
  };

  return (
    <div style={{ fontFamily: 'var(--font-mono)' }}>
      <h2 style={{ color: 'var(--accent-green)', textShadow: 'var(--glow-green)' }}>MEMORY SEARCH</h2>
      <div style={{ display: 'flex', gap: 'var(--spacing-sm)', marginBottom: 'var(--spacing-lg)' }}>
        <Input value={query} onChange={e => setQuery(e.target.value)} onKeyDown={e => e.key === 'Enter' && search()} placeholder="Search memories..." style={{ flex: 1 }} />
        <Button onClick={search}>SEARCH</Button>
      </div>
      {results.map(entry => (
        <Card key={entry.id}>
          <div style={{ color: 'var(--text-primary)' }}>{entry.content}</div>
          <div style={{ color: 'var(--accent-cyan)', fontSize: '11px' }}>Relevance: {(entry.relevance * 100).toFixed(1)}%</div>
        </Card>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route and commit**

```bash
git add synthia-web/src/pages/MemoryPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement MemoryPage with search functionality"
```

---

## Task 25: Jobs Page (Task 3.6)

**Files:**
- Create: `synthia-web/src/pages/JobsPage.tsx`

- [ ] **Step 1: Create JobsPage**

Create `synthia-web/src/pages/JobsPage.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';

interface Job {
  id: string;
  name: string;
  schedule: string;
  enabled: boolean;
}

export function JobsPage() {
  const [jobs, setJobs] = useState<Job[]>([]);

  useEffect(() => {
    fetch('/api/jobs')
      .then(r => r.json())
      .then(data => setJobs(data || []))
      .catch(() => setJobs([]));
  }, []);

  const toggleJob = async (id: string, enabled: boolean) => {
    await fetch(`/api/jobs/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled: !enabled }) });
    setJobs(prev => prev.map(j => j.id === id ? { ...j, enabled: !enabled } : j));
  };

  return (
    <div style={{ fontFamily: 'var(--font-mono)' }}>
      <h2 style={{ color: 'var(--accent-green)', textShadow: 'var(--glow-green)' }}>JOBS</h2>
      {jobs.length === 0 ? (
        <Card><div style={{ color: 'var(--text-secondary)' }}>No scheduled jobs</div></Card>
      ) : (
        jobs.map(job => (
          <Card key={job.id}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <div style={{ color: 'var(--accent-cyan)' }}>{job.name}</div>
                <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>{job.schedule}</div>
              </div>
              <Button variant={job.enabled ? 'primary' : 'secondary'} onClick={() => toggleJob(job.id, job.enabled)}>
                {job.enabled ? 'ACTIVE' : 'PAUSED'}
              </Button>
            </div>
          </Card>
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route and commit**

```bash
git add synthia-web/src/pages/JobsPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement JobsPage with job scheduling management"
```

---

## Task 26: MCP Page (Task 3.7)

**Files:**
- Create: `synthia-web/src/pages/McpPage.tsx`

- [ ] **Step 1: Create McpPage**

Create `synthia-web/src/pages/McpPage.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';

interface McpServer {
  id: string;
  name: string;
  url: string;
  status: string;
}

export function McpPage() {
  const [servers, setServers] = useState<McpServer[]>([]);

  useEffect(() => {
    fetch('/api/mcp/servers')
      .then(r => r.json())
      .then(data => setServers(data || []))
      .catch(() => setServers([]));
  }, []);

  const removeServer = async (id: string) => {
    await fetch(`/api/mcp/servers/${id}`, { method: 'DELETE' });
    setServers(prev => prev.filter(s => s.id !== id));
  };

  return (
    <div style={{ fontFamily: 'var(--font-mono)' }}>
      <h2 style={{ color: 'var(--accent-green)', textShadow: 'var(--glow-green)' }}>MCP SERVERS</h2>
      {servers.length === 0 ? (
        <Card><div style={{ color: 'var(--text-secondary)' }}>No MCP servers registered</div></Card>
      ) : (
        servers.map(server => (
          <Card key={server.id}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <div style={{ color: 'var(--accent-cyan)' }}>{server.name}</div>
                <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>{server.url}</div>
                <div style={{ color: server.status === 'connected' ? 'var(--accent-green)' : '#ff0055', fontSize: '11px' }}>{server.status.toUpperCase()}</div>
              </div>
              <Button variant="secondary" onClick={() => removeServer(server.id)}>REMOVE</Button>
            </div>
          </Card>
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 2: Update App.tsx route and commit**

```bash
git add synthia-web/src/pages/McpPage.tsx synthia-web/src/App.tsx
git commit -m "feat(web): implement McpPage with server management"
```

---

## Task 27: Phase 3 Validation (Task 3.8)

- [ ] **Step 1: Verify all pages**

Open browser at `http://localhost:5173`. Navigate to each page:
- `/chat` - Chat works
- `/tools` - Tools listed
- `/skills` - Skills toggle works
- `/settings` - Settings save works
- `/tasks` - Task history shown
- `/memory` - Search works
- `/jobs` - Job management works
- `/mcp` - MCP server management works

- [ ] **Step 2: Commit validation**

```bash
git commit --allow-empty -m "chore: validate Phase 3 - all management feature pages functional"
```

---

## Task 28: Playwright Setup (Task 4.1)

**Files:**
- Modify: `synthia-web/package.json`
- Create: `synthia-web/playwright.config.ts`

- [ ] **Step 1: Install Playwright**

Run: `cd synthia-web && npm install -D @playwright/test`

- [ ] **Step 2: Create playwright.config.ts**

Create `synthia-web/playwright.config.ts`:

```typescript
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  ],
  webServer: [
    {
      command: 'cd .. && cargo run -p synthia-server',
      port: 8080,
      reuseExistingServer: !process.env.CI,
    },
    {
      command: 'npm run dev',
      port: 5173,
      reuseExistingServer: !process.env.CI,
    },
  ],
});
```

- [ ] **Step 3: Commit**

```bash
git add synthia-web/package.json synthia-web/package-lock.json synthia-web/playwright.config.ts
git commit -m "feat(web): setup Playwright E2E testing framework"
```

---

## Task 29: Page Object Model Base (Task 4.2)

**Files:**
- Create: `synthia-web/tests/e2e/pages/base.page.ts`

- [ ] **Step 1: Create base page object**

Create `synthia-web/tests/e2e/pages/base.page.ts`:

```typescript
import { Page } from '@playwright/test';

export class BasePage {
  constructor(protected page: Page) {}

  async navigate(path: string) {
    await this.page.goto(path);
  }

  async waitForLoad() {
    await this.page.waitForLoadState('networkidle');
  }

  async getTitle() {
    return this.page.title();
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add synthia-web/tests/e2e/pages/base.page.ts
git commit -m "feat(web): create base Page Object Model class"
```

---

## Task 30: Chat Page Object (Task 4.3)

**Files:**
- Create: `synthia-web/tests/e2e/pages/chat.page.ts`

- [ ] **Step 1: Create chat page object**

Create `synthia-web/tests/e2e/pages/chat.page.ts`:

```typescript
import { Page } from '@playwright/test';
import { BasePage } from './base.page';

export class ChatPage extends BasePage {
  constructor(page: Page) {
    super(page);
  }

  async sendMessage(text: string) {
    await this.page.getByPlaceholder('Type a message...').fill(text);
    await this.page.getByRole('button', { name: 'SEND' }).click();
  }

  async getMessages() {
    return this.page.locator('[class*="Card"]').allTextContents();
  }

  async waitForResponse() {
    await this.page.waitForTimeout(2000);
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add synthia-web/tests/e2e/pages/chat.page.ts
git commit -m "feat(web): create ChatPage Page Object Model"
```

---

## Task 31: Other Page Objects (Task 4.4)

**Files:**
- Create: `synthia-web/tests/e2e/pages/tools.page.ts`
- Create: `synthia-web/tests/e2e/pages/skills.page.ts`
- Create: `synthia-web/tests/e2e/pages/settings.page.ts`

- [ ] **Step 1: Create page objects**

Create `synthia-web/tests/e2e/pages/tools.page.ts`:

```typescript
import { Page } from '@playwright/test';
import { BasePage } from './base.page';

export class ToolsPage extends BasePage {
  constructor(page: Page) { super(page); }
  async getToolCount() { return this.page.locator('[class*="Card"]').count(); }
}
```

Create `synthia-web/tests/e2e/pages/skills.page.ts`:

```typescript
import { Page } from '@playwright/test';
import { BasePage } from './base.page';

export class SkillsPage extends BasePage {
  constructor(page: Page) { super(page); }
  async toggleSkill(index: number) { await this.page.getByRole('button').nth(index).click(); }
}
```

Create `synthia-web/tests/e2e/pages/settings.page.ts`:

```typescript
import { Page } from '@playwright/test';
import { BasePage } from './base.page';

export class SettingsPage extends BasePage {
  constructor(page: Page) { super(page); }
  async setProvider(value: string) { await this.page.getByLabel('PROVIDER').fill(value); }
  async setModel(value: string) { await this.page.getByLabel('MODEL').fill(value); }
  async save() { await this.page.getByRole('button', { name: 'SAVE' }).click(); }
}
```

- [ ] **Step 2: Commit**

```bash
git add synthia-web/tests/e2e/pages/
git commit -m "feat(web): create Page Object Models for Tools, Skills, Settings pages"
```

---

## Task 32: Layer 1 UI Tests - Navigation (Task 4.5)

**Files:**
- Create: `synthia-web/tests/e2e/ui/navigation.spec.ts`

- [ ] **Step 1: Create navigation tests**

Create `synthia-web/tests/e2e/ui/navigation.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';

test.describe('Navigation', () => {
  test('should navigate to all pages via sidebar', async ({ page }) => {
    await page.goto('/');
    const links = ['CHAT', 'TOOLS', 'SKILLS', 'SETTINGS', 'TASKS', 'MEMORY', 'JOBS', 'MCP'];
    for (const link of links) {
      await page.getByText(link).click();
      await expect(page).toHaveURL(new RegExp(link.toLowerCase()));
    }
  });

  test('should redirect / to /chat', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveURL('/chat');
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd synthia-web && npx playwright test tests/e2e/ui/navigation.spec.ts`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add synthia-web/tests/e2e/ui/navigation.spec.ts
git commit -m "test(web): add Layer 1 UI navigation tests"
```

---

## Task 33: Layer 1 UI Tests - Chat (Task 4.6)

**Files:**
- Create: `synthia-web/tests/e2e/ui/chat-ui.spec.ts`

- [ ] **Step 1: Create chat UI tests**

Create `synthia-web/tests/e2e/ui/chat-ui.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

test.describe('Chat UI', () => {
  test('should display chat input and send button', async ({ page }) => {
    await page.goto('/chat');
    await expect(page.getByPlaceholder('Type a message...')).toBeVisible();
    await expect(page.getByRole('button', { name: 'SEND' })).toBeVisible();
  });

  test('should add user message to chat', async ({ page }) => {
    const chatPage = new ChatPage(page);
    await page.goto('/chat');
    await chatPage.sendMessage('Hello');
    const messages = await chatPage.getMessages();
    expect(messages.length).toBeGreaterThan(0);
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd synthia-web && npx playwright test tests/e2e/ui/chat-ui.spec.ts`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add synthia-web/tests/e2e/ui/chat-ui.spec.ts
git commit -m "test(web): add Layer 1 UI chat interaction tests"
```

---

## Task 34: Layer 2 Integration Tests - A2A Protocol (Task 4.7)

**Files:**
- Create: `synthia-web/tests/e2e/integration/a2a-protocol.spec.ts`

- [ ] **Step 1: Create A2A protocol tests**

Create `synthia-web/tests/e2e/integration/a2a-protocol.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

test.describe('A2A Protocol Integration', () => {
  test('should send message and receive streaming response', async ({ page }) => {
    const chatPage = new ChatPage(page);
    await page.goto('/chat');
    await chatPage.sendMessage('What is 2+2?');
    await chatPage.waitForResponse();
    const messages = await chatPage.getMessages();
    expect(messages.length).toBeGreaterThanOrEqual(2);
  });

  test('should show connection status', async ({ page }) => {
    await page.goto('/chat');
    const status = page.locator('text=CONNECTED');
    await expect(status).toBeVisible({ timeout: 5000 });
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd synthia-web && npx playwright test tests/e2e/integration/a2a-protocol.spec.ts`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add synthia-web/tests/e2e/integration/a2a-protocol.spec.ts
git commit -m "test(web): add Layer 2 A2A protocol integration tests"
```

---

## Task 35: Layer 2 Integration Tests - API CRUD (Task 4.8)

**Files:**
- Create: `synthia-web/tests/e2e/integration/api-crud.spec.ts`

- [ ] **Step 1: Create API CRUD tests**

Create `synthia-web/tests/e2e/integration/api-crud.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';
import { SettingsPage } from '../pages/settings.page';

test.describe('API CRUD Integration', () => {
  test('should save and load settings', async ({ page }) => {
    const settingsPage = new SettingsPage(page);
    await page.goto('/settings');
    await settingsPage.setProvider('test-provider');
    await settingsPage.setModel('test-model');
    await settingsPage.save();
    await page.reload();
    await expect(page.getByLabel('PROVIDER')).toHaveValue('test-provider');
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd synthia-web && npx playwright test tests/e2e/integration/api-crud.spec.ts`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add synthia-web/tests/e2e/integration/api-crud.spec.ts
git commit -m "test(web): add Layer 2 API CRUD integration tests"
```

---

## Task 36: Layer 3 Agent Tests - Conversation (Task 4.9)

**Files:**
- Create: `synthia-web/tests/e2e/agent/conversation.spec.ts`

- [ ] **Step 1: Create agent conversation tests**

Create `synthia-web/tests/e2e/agent/conversation.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';
import { ChatPage } from '../pages/chat.page';

test.describe('Agent Conversation', () => {
  test('should handle multi-turn conversation', async ({ page }) => {
    const chatPage = new ChatPage(page);
    await page.goto('/chat');
    await chatPage.sendMessage('Hello');
    await chatPage.waitForResponse();
    await chatPage.sendMessage('Follow up question');
    await chatPage.waitForResponse();
    const messages = await chatPage.getMessages();
    expect(messages.length).toBeGreaterThanOrEqual(4);
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd synthia-web && npx playwright test tests/e2e/agent/conversation.spec.ts`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add synthia-web/tests/e2e/agent/conversation.spec.ts
git commit -m "test(web): add Layer 3 agent conversation tests"
```

---

## Task 37: Layer 3 Agent Tests - Task Lifecycle (Task 4.10)

**Files:**
- Create: `synthia-web/tests/e2e/agent/task-lifecycle.spec.ts`

- [ ] **Step 1: Create task lifecycle tests**

Create `synthia-web/tests/e2e/agent/task-lifecycle.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';

test.describe('Task Lifecycle', () => {
  test('should show task in tasks page after sending message', async ({ page }) => {
    await page.goto('/chat');
    await page.getByPlaceholder('Type a message...').fill('Test task');
    await page.getByRole('button', { name: 'SEND' }).click();
    await page.waitForTimeout(2000);
    await page.goto('/tasks');
    await expect(page.locator('text=Task:')).toBeVisible({ timeout: 5000 });
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd synthia-web && npx playwright test tests/e2e/agent/task-lifecycle.spec.ts`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add synthia-web/tests/e2e/agent/task-lifecycle.spec.ts
git commit -m "test(web): add Layer 3 task lifecycle tests"
```

---

## Task 38: Phase 4 Validation (Task 4.11)

- [ ] **Step 1: Run all E2E tests**

Run: `cd synthia-web && npx playwright test`
Expected: All tests pass

- [ ] **Step 2: Commit validation**

```bash
git commit --allow-empty -m "chore: validate Phase 4 - all three E2E test layers passing"
```

---

## Task 39: Docker Compose Dev (Task 5.1)

**Files:**
- Create: `docker-compose.yml`

- [ ] **Step 1: Create docker-compose.yml**

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  server:
    build:
      context: .
      dockerfile: Dockerfile.server
    ports:
      - "8080:8080"
    environment:
      - RUST_LOG=info
    volumes:
      - ./config.yaml:/app/config.yaml

  web:
    build:
      context: .
      dockerfile: Dockerfile.web
    ports:
      - "80:80"
    depends_on:
      - server
```

- [ ] **Step 2: Commit**

```bash
git add docker-compose.yml
git commit -m "feat: add docker-compose.yml for development environment"
```

---

## Task 40: Docker Compose Production (Task 5.2)

**Files:**
- Create: `docker-compose.prod.yml`

- [ ] **Step 1: Create production docker-compose**

Create `docker-compose.prod.yml`:

```yaml
version: '3.8'

services:
  server:
    build:
      context: .
      dockerfile: Dockerfile.server
    ports:
      - "8080:8080"
    environment:
      - RUST_LOG=warn
    restart: always

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
    volumes:
      - ./nginx.conf:/etc/nginx/conf.d/default.conf
      - ./synthia-web/dist:/usr/share/nginx/html
    depends_on:
      - server
    restart: always
```

- [ ] **Step 2: Commit**

```bash
git add docker-compose.prod.yml
git commit -m "feat: add docker-compose.prod.yml for production deployment"
```

---

## Task 41: Nginx Configuration (Tasks 5.3 & 5.4)

**Files:**
- Create: `nginx.conf`

- [ ] **Step 1: Create nginx.conf with SSE support**

Create `nginx.conf`:

```nginx
server {
    listen 80;
    server_name localhost;
    root /usr/share/nginx/html;
    index index.html;

    # SPA fallback
    location / {
        try_files $uri $uri/ /index.html;
    }

    # Proxy API to backend
    location /api/ {
        proxy_pass http://server:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # Proxy A2A to backend with SSE support
    location /a2a/ {
        proxy_pass http://server:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }

    # Health check proxy
    location /health {
        proxy_pass http://server:8080;
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add nginx.conf
git commit -m "feat: add Nginx reverse proxy config with SSE streaming support"
```

---

## Task 42: Docker Dev Validation (Task 5.5)

- [ ] **Step 1: Start Docker Compose dev**

Run: `docker-compose up -d`
Expected: Both services start

- [ ] **Step 2: Verify access**

Open browser at `http://localhost`. Verify frontend loads and can communicate with backend.

- [ ] **Step 3: Stop and commit**

Run: `docker-compose down`

```bash
git commit --allow-empty -m "chore: validate Docker Compose dev environment"
```

---

## Task 43: Docker Prod Validation (Task 5.6)

- [ ] **Step 1: Build frontend for production**

Run: `make build-web`

- [ ] **Step 2: Start production Docker Compose**

Run: `docker-compose -f docker-compose.prod.yml up -d`
Expected: Both services start

- [ ] **Step 3: Verify access**

Open browser at `http://localhost`. Verify frontend loads and communicates with backend through Nginx.

- [ ] **Step 4: Stop and commit**

Run: `docker-compose -f docker-compose.prod.yml down`

```bash
git commit --allow-empty -m "chore: validate Docker Compose production deployment"
```

---

## Task 44: README Update (Task 5.7)

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update README with full-stack instructions**

Add/update these sections in `README.md`:

```markdown
## Development

```bash
make dev          # Start frontend (:5173) and backend (:8080)
make build        # Build both frontend and backend
make test         # Run all tests
make test-e2e     # Run E2E tests
```

## Deployment

### Separate Deployment (Recommended)

```bash
# Build
make build-web     # Build frontend
make build-server  # Build backend

# Docker
docker-compose -f docker-compose.prod.yml up -d
```

### Architecture

- Frontend: React + Vite, served by Nginx
- Backend: Rust + Axum, serves A2A protocol and management API
- Communication: A2A protocol (JSON-RPC over HTTP/SSE)
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README with full-stack development and deployment instructions"
```

---

## Task 45: Deployment Guide (Task 5.8)

**Files:**
- Create: `DEPLOYMENT.md`

- [ ] **Step 1: Create DEPLOYMENT.md**

Create `DEPLOYMENT.md`:

```markdown
# Synthia Deployment Guide

## Prerequisites
- Rust 1.95+
- Node.js 20+
- Docker & Docker Compose (optional)

## Local Development

```bash
make dev    # Starts backend on :8080 and frontend on :5173
```

## Production Deployment

### Option 1: Docker Compose (Recommended)

```bash
make build-web
docker-compose -f docker-compose.prod.yml up -d
```

### Option 2: Manual

1. Build backend: `cargo build --release -p synthia-server`
2. Build frontend: `cd synthia-web && npm run build`
3. Serve frontend with Nginx using `nginx.conf`
4. Run backend: `./target/release/synthia-server`

## Architecture

- Frontend served by Nginx on port 80
- Backend serves API on port 8080
- Nginx proxies `/api/` and `/a2a/` to backend
- SSE streaming supported via `proxy_buffering off`
```

- [ ] **Step 2: Commit**

```bash
git add DEPLOYMENT.md
git commit -m "docs: add deployment guide"
```

---

## Task 46: Final Validation

- [ ] **Step 1: Run full test suite**

Run: `make test`
Expected: All Rust and frontend tests pass

- [ ] **Step 2: Run E2E tests**

Run: `make test-e2e`
Expected: All E2E tests pass

- [ ] **Step 3: Final commit**

```bash
git commit --allow-empty -m "chore: full-stack integration complete - all phases validated"
```
