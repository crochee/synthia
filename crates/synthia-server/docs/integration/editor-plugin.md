---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 编辑器插件集成

## 1. 概述

本文档说明如何开发 Synthia Agent 的编辑器插件，支持 VS Code、JetBrains IDE 等主流编辑器。

## 2. VS Code 扩展

### 2.1 项目结构

```
synthia-vscode/
├── src/
│   ├── extension.ts          # 扩展入口
│   ├── agentClient.ts        # Agent 客户端
│   ├── chatProvider.ts       # 聊天视图
│   ├── codeActionProvider.ts # 代码操作
│   └── utils.ts              # 工具函数
├── package.json              # 扩展配置
├── tsconfig.json
└── README.md
```

### 2.2 扩展配置

```json
{
  "name": "synthia-agent",
  "displayName": "Synthia Agent",
  "description": "AI-powered coding assistant",
  "version": "0.1.0",
  "engines": {
    "vscode": "^1.74.0"
  },
  "categories": ["Programming Languages", "Machine Learning"],
  "activationEvents": [
    "onCommand:synthia.chat",
    "onView:synthia.chatView"
  ],
  "main": "./out/extension.js",
  "contributes": {
    "commands": [
      {
        "command": "synthia.chat",
        "title": "Open Chat"
      },
      {
        "command": "synthia.reviewCode",
        "title": "Review Code"
      }
    ],
    "viewsContainers": {
      "activitybar": [
        {
          "id": "synthia",
          "title": "Synthia Agent",
          "icon": "resources/icon.svg"
        }
      ]
    },
    "views": {
      "synthia": [
        {
          "id": "synthia.chatView",
          "name": "Chat"
        }
      ]
    },
    "configuration": {
      "title": "Synthia Agent",
      "properties": {
        "synthia.apiUrl": {
          "type": "string",
          "default": "http://localhost:8080",
          "description": "Synthia API URL"
        },
        "synthia.apiKey": {
          "type": "string",
          "description": "API Key for authentication"
        }
      }
    }
  }
}
```

### 2.3 扩展入口

```typescript
import * as vscode from 'vscode';
import { AgentClient } from './agentClient';
import { ChatViewProvider } from './chatProvider';

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('synthia');
  const apiUrl = config.get<string>('apiUrl') || 'http://localhost:8080';
  const apiKey = config.get<string>('apiKey') || '';

  const client = new AgentClient(apiUrl, apiKey);
  const chatProvider = new ChatViewProvider(context.extensionUri, client);

  // 注册聊天视图
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(
      'synthia.chatView',
      chatProvider
    )
  );

  // 注册命令
  context.subscriptions.push(
    vscode.commands.registerCommand('synthia.chat', () => {
      vscode.commands.executeCommand('workbench.view.extension.synthia');
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('synthia.reviewCode', async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showErrorMessage('No active editor');
        return;
      }

      const selection = editor.selection;
      const code = editor.document.getText(selection);

      await client.sendChat(`请审查这段代码:\n\`\`\`\n${code}\n\`\`\``);
    })
  );

  // 注册代码操作提供者
  context.subscriptions.push(
    vscode.languages.registerCodeActionsProvider(
      { scheme: 'file' },
      new CodeActionProvider(client),
      { providedCodeActionKinds: [vscode.CodeActionKind.QuickFix] }
    )
  );
}

export function deactivate() {}
```

### 2.4 Agent 客户端

```typescript
import axios from 'axios';

export class AgentClient {
  private client;
  private sessionId?: string;

  constructor(
    private apiUrl: string,
    private apiKey: string
  ) {
    this.client = axios.create({
      baseURL: apiUrl,
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${apiKey}`,
      },
    });
  }

  async sendChat(message: string, agent?: string): Promise<string> {
    const response = await this.client.post('/chat', {
      message,
      session_id: this.sessionId,
      agent,
    });

    this.sessionId = response.data.session_id;
    return response.data.message.content;
  }

  async *streamChat(message: string, agent?: string): AsyncGenerator<string> {
    const response = await fetch(`${this.apiUrl}/chat/stream`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${this.apiKey}`,
      },
      body: JSON.stringify({
        message,
        session_id: this.sessionId,
        agent,
      }),
    });

    const reader = response.body?.getReader();
    const decoder = new TextDecoder();

    if (!reader) return;

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      const chunk = decoder.decode(value);
      const lines = chunk.split('\n').filter(line => line.trim());

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          const data = JSON.parse(line.slice(6));
          if (data.content) {
            yield data.content;
          }
        }
      }
    }
  }
}
```

### 2.5 聊天视图

```typescript
import * as vscode from 'vscode';
import { AgentClient } from './agentClient';

export class ChatViewProvider implements vscode.WebviewViewProvider {
  private _view?: vscode.WebviewView;

  constructor(
    private readonly _extensionUri: vscode.Uri,
    private readonly _client: AgentClient
  ) {}

  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ) {
    this._view = webviewView;

    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this._extensionUri],
    };

    webviewView.webview.html = this._getHtmlForWebview(webviewView.webview);

    webviewView.webview.onDidReceiveMessage(async (data) => {
      switch (data.type) {
        case 'sendMessage':
          await this._handleMessage(data.message);
          break;
      }
    });
  }

  private async _handleMessage(message: string) {
    if (!this._view) return;

    // 显示用户消息
    this._view.webview.postMessage({
      type: 'addMessage',
      message: { role: 'user', content: message },
    });

    // 流式接收响应
    let assistantMessage = '';
    for await (const chunk of this._client.streamChat(message)) {
      assistantMessage += chunk;
      this._view.webview.postMessage({
        type: 'updateMessage',
        message: { role: 'assistant', content: assistantMessage },
      });
    }
  }

  private _getHtmlForWebview(webview: vscode.Webview): string {
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Synthia Chat</title>
  <style>
    body { font-family: var(--vscode-font-family); }
    .messages { height: calc(100vh - 100px); overflow-y: auto; }
    .message { margin: 8px; padding: 8px; border-radius: 4px; }
    .user { background: var(--vscode-input-background); }
    .assistant { background: var(--vscode-editor-background); }
    .input-area { position: fixed; bottom: 0; width: 100%; }
    input { width: calc(100% - 60px); }
    button { width: 50px; }
  </style>
</head>
<body>
  <div class="messages" id="messages"></div>
  <div class="input-area">
    <input type="text" id="input" placeholder="Type a message...">
    <button onclick="sendMessage()">Send</button>
  </div>

  <script>
    const vscode = acquireVsCodeApi();
    const messagesDiv = document.getElementById('messages');
    const input = document.getElementById('input');

    function sendMessage() {
      const message = input.value.trim();
      if (!message) return;

      vscode.postMessage({ type: 'sendMessage', message });
      input.value = '';
    }

    input.addEventListener('keypress', (e) => {
      if (e.key === 'Enter') sendMessage();
    });

    window.addEventListener('message', (event) => {
      const data = event.data;
      switch (data.type) {
        case 'addMessage':
        case 'updateMessage':
          updateMessage(data.message);
          break;
      }
    });

    function updateMessage(message) {
      const div = document.createElement('div');
      div.className = 'message ' + message.role;
      div.textContent = message.content;
      messagesDiv.appendChild(div);
      messagesDiv.scrollTop = messagesDiv.scrollHeight;
    }
  </script>
</body>
</html>`;
  }
}
```

## 3. JetBrains 插件

### 3.1 项目结构

```
synthia-jetbrains/
├── src/
│   └── main/
│       ├── kotlin/
│       │   └── com/synthia/
│       │       ├── SynthiaPlugin.kt        # 插件入口
│       │       ├── AgentClient.kt          # Agent 客户端
│       │       └── ChatToolWindow.kt       # 聊天窗口
│       └── resources/
│           └── META-INF/
│               └── plugin.xml              # 插件配置
└── build.gradle.kts
```

### 3.2 插件配置

```xml
<idea-plugin>
  <id>com.synthia.agent</id>
  <name>Synthia Agent</name>
  <version>0.1.0</version>
  <vendor email="support@synthia.com" url="https://synthia.com">Synthia</vendor>

  <description>AI-powered coding assistant</description>

  <depends>com.intellij.modules.platform</depends>

  <extensions defaultExtensionNs="com.intellij">
    <toolWindow id="Synthia"
                secondary="true"
                icon="/META-INF/icon.svg"
                anchor="right"
                factoryClass="com.synthia.ChatToolWindowFactory"/>
    
    <applicationConfigurable
      parentId="tools"
      instance="com.synthia.SynthiaConfigurable"
      id="com.synthia.SynthiaConfigurable"
      displayName="Synthia Agent"/>
  </extensions>

  <actions>
    <action id="Synthia.ReviewCode"
            class="com.synthia.ReviewCodeAction"
            text="Review Code"
            description="Review selected code with Synthia">
      <add-to-group group-id="EditorPopupMenu" anchor="first"/>
    </action>
  </actions>
</idea-plugin>
```

### 3.3 插件实现

```kotlin
package com.synthia

import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.ui.Messages

class ReviewCodeAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR) ?: return
        
        val selectionModel = editor.selectionModel
        val selectedText = selectionModel.selectedText
        
        if (selectedText.isNullOrBlank()) {
            Messages.showWarningDialog(
                project,
                "Please select some code to review",
                "No Selection"
            )
            return
        }
        
        val client = project.getService(AgentClient::class.java)
        client.sendChat("请审查这段代码:\n```\n$selectedText\n```")
    }
}
```

## 4. LSP 集成

### 4.1 LSP 服务器

```typescript
import {
  createConnection,
  TextDocuments,
  ProposedFeatures,
  InitializeParams,
  DidChangeConfigurationNotification,
  CompletionItem,
  CompletionItemKind,
  TextDocumentPositionParams,
  TextDocumentSyncKind,
  InitializeResult,
} from 'vscode-languageserver/node';

import { TextDocument } from 'vscode-languageserver-textdocument';

const connection = createConnection(ProposedFeatures.all);
const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument);

let hasConfigurationCapability = false;
let hasWorkspaceFolderCapability = false;

connection.onInitialize((params: InitializeParams) => {
  const capabilities = params.capabilities;

  hasConfigurationCapability = !!(
    capabilities.workspace && !!capabilities.workspace.configuration
  );
  hasWorkspaceFolderCapability = !!(
    capabilities.workspace && !!capabilities.workspace.workspaceFolders
  );

  const result: InitializeResult = {
    capabilities: {
      textDocumentSync: TextDocumentSyncKind.Incremental,
      completionProvider: {
        resolveProvider: true,
      },
      codeActionProvider: true,
    },
  };

  return result;
});

connection.onCodeAction(async (params) => {
  const document = documents.get(params.textDocument.uri);
  if (!document) return [];

  const range = params.range;
  const text = document.getText(range);

  // 调用 Agent 获取代码建议
  const suggestions = await getAgentSuggestions(text);

  return suggestions.map((suggestion, index) => ({
    title: suggestion.title,
    kind: CodeActionKind.QuickFix,
    edit: {
      changes: {
        [params.textDocument.uri]: [
          {
            range,
            newText: suggestion.newText,
          },
        ],
      },
    },
  }));
});

documents.listen(connection);
connection.listen();
```

## 5. 最佳实践

### 5.1 性能优化

1. **异步操作**：所有网络请求使用异步
2. **缓存结果**：缓存常用请求结果
3. **节流防抖**：对频繁操作进行节流
4. **懒加载**：按需加载资源

### 5.2 用户体验

1. **即时反馈**：提供加载状态和进度
2. **错误提示**：清晰的错误信息
3. **快捷键**：支持常用快捷键
4. **主题适配**：适配编辑器主题

### 5.3 安全性

1. **API 密钥保护**：安全存储 API 密钥
2. **输入验证**：验证所有用户输入
3. **HTTPS**：使用 HTTPS 通信
4. **权限控制**：最小权限原则

## 6. 相关文档

- [前端集成](frontend-integration.md)
- [API使用指南](../api-reference/API_GUIDE.md)

## 7. 参考资料

- [VS Code Extension API](https://code.visualstudio.com/api)
- [JetBrains Plugin Development](https://plugins.jetbrains.com/docs/intellij/)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
