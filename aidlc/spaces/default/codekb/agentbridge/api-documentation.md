# API Documentation — agentbridge

## Capability Traits (`core/platform.rs`) — 引擎↔平台契约

| Trait | 关键方法 | 默认 |
|-------|---------|------|
| `Platform`(必需) | `name()`, `start(MessageHandler)`, `reply(ctx,content)`, `send(ctx,content)`, `reply_quoted`(默认→reply), `register_commands(&[BotCommand])`(默认 no-op), `stop()` | — |
| `PlatformCapabilities: Platform` | `as_message_updater/as_image_sender/as_file_sender/as_inline_button_sender/as_typing_indicator -> Option<&dyn _>` | 全 `None` |
| `MessageUpdater` | `send_preview -> Box<dyn PreviewHandle>`, `update_preview`, `delete_preview` | — |
| `ImageSender` | `send_image(ctx,&[u8],filename,mime)` | — |
| `FileSender` | `send_file(...)` | 定义但无适配器实现(dead contract) |
| `InlineButtonSender` | `send_with_buttons(ctx,text,&[Button]) -> Box<dyn PreviewHandle>`, `answer_callback` | — |
| `TypingIndicator` | `start_typing(ctx) -> Box<dyn FnOnce()+Send>`(调用即停) | — |

支持类型:`ReplyCtx`(opaque,`as_any()`/`session_key_hint()`/`clone_box()`)、`PreviewHandle`(opaque)、`Button{text,callback_data}`、`BotCommand{name,description}`、`MessageHandler = Arc<dyn Fn(Arc<dyn PlatformCapabilities>, IncomingMessage)+Send+Sync>`。

## AgentSession Trait (`agent/mod.rs`) — 引擎↔agent 契约

`send(prompt)`, `send_with_attachments(prompt,images,files)`, `respond_permission(request_id,allow)`, `permission_responder() -> Arc<dyn PermissionResponder>`, `take_events()/replace_events()/events()/drain_stale_events()`(mpsc Receiver<AgentEvent>), `session_id() -> Option<String>`, `alive() -> bool`, `close()`。

## AgentEvent 枚举 (`core/event.rs`) — 事件契约【hook-relay 关键】

```
System { session_id, tools: Vec<String>, skills: Vec<String> }
Text { content: String }
Thinking { content: String }
ToolUse { id, tool, input: String }
ToolResult { id, output, is_error: bool }
PermissionRequest { request_id, tool, input: Value, options: Vec<PermissionOption> }
Result { content, session_id, input_tokens: u32, output_tokens: u32 }
Image { data: Vec<u8>, filename, mime }   // 未提交,tmux 截图路径(本 intent 将移除)
Error { message }
```

**hook-relay 映射方向**:Stop hook 的 `last_assistant_message` → `Text`/`Result`;PostToolUse 的 `tool_name`/`tool_response` → `ToolUse`(经节流);无需新增枚举变体(也许复用现有 `ToolUse` 或加一个轻量进度变体,留 application-design 定)。

## Gateway HTTP/WS (`gateway/server.rs`)

- WS `GET /gateway/ws`(实例反向连,header `x-gateway-token`,30s ping);`GET /api/ws`(前端,首帧 `FrontendMessage::Auth{token}`)。
- REST(Bearer `api_token`):`GET /api/instances`、`POST /api/instances/{id}/send|command|permission`、`GET /api/instances/{id}/history`。10 MiB body,permissive CORS。

## 本地接收端(本 intent 新增,参考点)

webhook.rs 已是一个 axum HTTP 接收端(默认 9111)的现成范例 —— hook 接收端可借鉴其结构,但服务于 hook payload。
