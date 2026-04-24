import { defineStore } from 'pinia'
import { useAuthStore } from './auth'
import { useToast } from '~/composables/useToast'

function storedToChat(m: any): ChatMessage {
  const ts = new Date(m.created_at).getTime()
  const type: ChatMessage['type'] =
    m.role === 'user' ? 'user'
    : m.role === 'assistant' ? 'assistant'
    : m.role === 'tool' || m.role === 'tool_result' ? 'tool'
    : 'system'
  if (m.role === 'tool') {
    return {
      id: `hist-${m.id}`,
      type: 'tool',
      content: '',
      timestamp: ts,
      toolId: undefined,
      toolName: m.tool_name || 'tool',
      toolInput: m.content,
      toolHasResult: false,
    }
  }
  if (m.role === 'tool_result') {
    return {
      id: `hist-${m.id}`,
      type: 'tool',
      content: '',
      timestamp: ts,
      toolName: 'result',
      toolInput: '',
      toolOutput: m.content,
      toolIsError: false,
      toolHasResult: true,
    }
  }
  const content = m.role === 'thinking' ? `🧠 ${m.content}`
    : m.role === 'error' ? `💥 ${m.content}`
    : m.content
  return { id: `hist-${m.id}`, type, content, timestamp: ts }
}

interface SessionInfo {
  session_key: string
  session_id: string
  name: string | null
  agent_session_id: string | null
  updated_at: string
  is_busy: boolean
}

interface ProjectInfo {
  name: string
  work_dir: string
  sessions: SessionInfo[]
}

interface InstanceInfo {
  instance_id: string
  instance_name: string
  online: boolean
  projects: ProjectInfo[]
}

interface AgentEvent {
  type: string
  id?: string
  content?: string
  tool?: string
  input?: string
  output?: string
  message?: string
  is_error?: boolean
  input_tokens?: number
  output_tokens?: number
  request_id?: string
}

interface ChatMessage {
  id: string
  type: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  timestamp: number
  // For tool messages: pair tool_use with its tool_result
  toolId?: string
  toolName?: string
  toolInput?: string
  toolOutput?: string
  toolIsError?: boolean
  toolHasResult?: boolean
}

export const useGatewayStore = defineStore('gateway', {
  state: () => ({
    instances: [] as InstanceInfo[],
    // Currently selected session
    activeInstance: null as string | null,
    activeSession: null as string | null,
    // Chat messages per session (keyed by "instance:session_key")
    messages: {} as Record<string, ChatMessage[]>,
    // Whether more history exists before oldest message
    hasMoreHistory: {} as Record<string, boolean>,
    // Loading state for "load older"
    loadingMoreHistory: false,
    // WebSocket connection
    ws: null as WebSocket | null,
    wsConnected: false,
    // Loading states
    loadingInstances: false,
    loadingHistory: false,
    // Reconnect backoff
    reconnectAttempt: 0,
    reconnectTimer: null as ReturnType<typeof setTimeout> | null,
  }),

  getters: {
    activeMessages(state): ChatMessage[] {
      if (!state.activeInstance || !state.activeSession) return []
      const key = `${state.activeInstance}:${state.activeSession}`
      return state.messages[key] || []
    },
  },

  actions: {
    // Connect to gateway WebSocket
    connect() {
      const auth = useAuthStore()
      if (!auth.token) return

      // Cancel any pending reconnect
      if (this.reconnectTimer) {
        clearTimeout(this.reconnectTimer)
        this.reconnectTimer = null
      }

      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      const host = window.location.host
      const url = `${protocol}//${host}/api/ws`

      this.ws = new WebSocket(url)

      this.ws.onopen = () => {
        console.log('[WS] connected, sending auth')
        this.ws?.send(JSON.stringify({ type: 'auth', token: auth.token }))
      }

      this.ws.onmessage = (event) => {
        const data = JSON.parse(event.data)
        this.handleMessage(data)
      }

      this.ws.onclose = () => {
        this.wsConnected = false
        this.fetchInstances() // Keep data fresh via REST
        this.scheduleReconnect()
      }

      this.ws.onerror = (e) => {
        console.error('[WS] error:', e)
        this.wsConnected = false
        // Note: onclose fires right after, which handles reconnect.
      }
    },

    // Exponential backoff: 3s → 6s → 12s → 24s → 30s (cap)
    scheduleReconnect() {
      this.reconnectAttempt += 1
      const delayMs = Math.min(3000 * Math.pow(2, this.reconnectAttempt - 1), 30000)
      console.log(`[WS] reconnecting in ${delayMs}ms (attempt ${this.reconnectAttempt})`)
      this.reconnectTimer = setTimeout(() => {
        this.reconnectTimer = null
        this.connect()
      }, delayMs)
    },

    handleMessage(msg: any) {
      switch (msg.type) {
        case 'auth_ok':
          this.wsConnected = true
          this.reconnectAttempt = 0
          this.fetchInstances()
          break

        case 'auth_fail':
          const auth = useAuthStore()
          const toast = useToast()
          toast.error('Session expired, please login again')
          auth.logout()
          break

        case 'instance_online':
          this.instances = this.instances.filter(i => i.instance_id !== msg.instance_id)
          this.instances.push({
            instance_id: msg.instance_id,
            instance_name: msg.instance_name,
            online: true,
            projects: msg.projects || [],
          })
          break

        case 'instance_offline':
          const inst = this.instances.find(i => i.instance_id === msg.instance_id)
          if (inst) inst.online = false
          break

        case 'session_update':
          const target = this.instances.find(i => i.instance_id === msg.instance_id)
          if (target) target.projects = msg.projects
          break

        case 'event':
          this.handleEvent(msg.instance_id, msg.session_key, msg.event)
          break

        case 'user_message':
          this.handleUserMessage(msg.instance_id, msg.session_key, msg.text, msg.message_id)
          break
      }
    },

    handleUserMessage(instanceId: string, sessionKey: string, text: string, messageId: number) {
      const key = `${instanceId}:${sessionKey}`
      if (!this.messages[key]) this.messages[key] = []
      const msgs = this.messages[key]

      // Dedup: if our own optimistic user-message with same text is the last
      // user entry, upgrade its id to the server id instead of adding a dupe.
      for (let i = msgs.length - 1; i >= 0; i--) {
        const m = msgs[i]
        if (m.type === 'user') {
          if (m.content === text && m.id.startsWith('user-')) {
            m.id = `hist-${messageId}`
            return
          }
          // Different user text: stop scanning — this was a separate send.
          break
        }
      }

      // Not our own send; add it (another tab/device sent this).
      msgs.push({
        id: `hist-${messageId}`,
        type: 'user',
        content: text,
        timestamp: Date.now(),
      })
    },

    handleEvent(instanceId: string, sessionKey: string, event: AgentEvent) {
      const key = `${instanceId}:${sessionKey}`
      if (!this.messages[key]) this.messages[key] = []

      const msgs = this.messages[key]
      const id = `${Date.now()}-${Math.random().toString(36).slice(2, 6)}`
      const ts = Date.now()

      switch (event.type) {
        case 'text':
          // Append to last assistant message or create new
          const last = msgs[msgs.length - 1]
          if (last && last.type === 'assistant') {
            last.content += event.content || ''
          } else {
            msgs.push({ id, type: 'assistant', content: event.content || '', timestamp: ts })
          }
          break

        case 'thinking':
          msgs.push({ id, type: 'system', content: `🧠 ${event.content}`, timestamp: ts })
          break

        case 'tool_use':
          msgs.push({
            id,
            type: 'tool',
            content: '',
            timestamp: ts,
            toolId: event.id,
            toolName: event.tool,
            toolInput: event.input,
            toolHasResult: false,
          })
          break

        case 'tool_result': {
          // Pair with matching tool_use by id, falling back to the latest
          // tool card that has no result yet (older backends without id).
          const matchById = event.id
            ? [...msgs].reverse().find(m => m.type === 'tool' && m.toolId === event.id && !m.toolHasResult)
            : undefined
          const matchLast = matchById
            ? undefined
            : [...msgs].reverse().find(m => m.type === 'tool' && !m.toolHasResult)
          const target = matchById || matchLast
          if (target) {
            target.toolOutput = event.output
            target.toolIsError = event.is_error
            target.toolHasResult = true
          } else {
            // Orphan result (shouldn't happen, but be safe)
            const icon = event.is_error ? '💥' : '✓'
            msgs.push({
              id,
              type: 'tool',
              content: `${icon} ${event.output || ''}`,
              timestamp: ts,
            })
          }
          break
        }

        case 'result':
          // Result finalizes the turn — if content differs from accumulated text, replace
          if (event.content) {
            const lastMsg = msgs[msgs.length - 1]
            if (lastMsg && lastMsg.type === 'assistant') {
              lastMsg.content = event.content
            } else {
              msgs.push({ id, type: 'assistant', content: event.content, timestamp: ts })
            }
          }
          if (event.input_tokens) {
            msgs.push({
              id: id + '-tokens',
              type: 'system',
              content: `[tokens: in=${event.input_tokens} out=${event.output_tokens}]`,
              timestamp: ts,
            })
          }
          break

        case 'error':
          msgs.push({ id, type: 'system', content: `💥 ${event.message}`, timestamp: ts })
          break

        case 'permission_request':
          msgs.push({
            id,
            type: 'system',
            content: `🔐 需要确认: ${event.tool} (${event.request_id})`,
            timestamp: ts,
          })
          break
      }
    },

    // REST API calls
    async fetchInstances() {
      const auth = useAuthStore()
      const toast = useToast()
      this.loadingInstances = true
      try {
        const res = await fetch('/api/instances', {
          headers: { Authorization: `Bearer ${auth.token}` },
        })
        if (res.status === 401 || res.status === 403) {
          toast.error('Authentication expired, please login again')
          auth.logout()
          return
        }
        if (!res.ok) {
          toast.error(`Failed to load instances (HTTP ${res.status})`)
          return
        }
        const data = await res.json()
        this.instances = data.instances
      } catch (e: any) {
        toast.error(`Can't reach gateway: ${e?.message || e}`)
      } finally {
        this.loadingInstances = false
      }
    },

    async sendMessage(text: string) {
      if (!this.activeInstance || !this.activeSession) return
      const auth = useAuthStore()
      const toast = useToast()

      // Add user message to chat immediately
      const key = `${this.activeInstance}:${this.activeSession}`
      if (!this.messages[key]) this.messages[key] = []
      this.messages[key].push({
        id: `user-${Date.now()}`,
        type: 'user',
        content: text,
        timestamp: Date.now(),
      })

      try {
        const res = await fetch(`/api/instances/${this.activeInstance}/send`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            Authorization: `Bearer ${auth.token}`,
          },
          body: JSON.stringify({ session_key: this.activeSession, text }),
        })
        if (!res.ok) {
          toast.error(`Failed to send message (HTTP ${res.status})`)
        }
      } catch (e: any) {
        toast.error(`Can't send message: ${e?.message || e}`)
      }
    },

    async sendCommand(command: string) {
      if (!this.activeInstance || !this.activeSession) return
      const auth = useAuthStore()
      const toast = useToast()

      try {
        const res = await fetch(`/api/instances/${this.activeInstance}/command`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            Authorization: `Bearer ${auth.token}`,
          },
          body: JSON.stringify({ session_key: this.activeSession, command }),
        })
        if (!res.ok) {
          toast.error(`Command failed (HTTP ${res.status})`)
        }
      } catch (e: any) {
        toast.error(`Can't send command: ${e?.message || e}`)
      }
    },

    async selectSession(instanceId: string, sessionKey: string) {
      this.activeInstance = instanceId
      this.activeSession = sessionKey

      // Load chat history from gateway
      const key = `${instanceId}:${sessionKey}`
      if (!this.messages[key] || this.messages[key].length === 0) {
        await this.loadHistory(instanceId, sessionKey)
      }
    },

    async loadHistory(instanceId: string, sessionKey: string) {
      const auth = useAuthStore()
      const toast = useToast()
      const key = `${instanceId}:${sessionKey}`
      this.loadingHistory = true

      try {
        const res = await fetch(
          `/api/instances/${instanceId}/history?session_key=${encodeURIComponent(sessionKey)}&limit=50`,
          { headers: { Authorization: `Bearer ${auth.token}` } },
        )
        if (!res.ok) {
          toast.error(`Failed to load history (HTTP ${res.status})`)
          return
        }
        const data = await res.json()
        this.messages[key] = data.messages.map(storedToChat)
        this.hasMoreHistory[key] = !!data.has_more
      } catch (e: any) {
        toast.error(`Can't load history: ${e?.message || e}`)
      } finally {
        this.loadingHistory = false
      }
    },

    async loadOlderHistory(instanceId: string, sessionKey: string) {
      const auth = useAuthStore()
      const toast = useToast()
      const key = `${instanceId}:${sessionKey}`
      const existing = this.messages[key] || []
      if (existing.length === 0 || this.loadingMoreHistory) return
      // Use the oldest known id as the cursor
      const oldest = existing[0]
      const cursor = oldest.id.startsWith('hist-') ? oldest.id.slice(5) : null
      if (!cursor) return

      this.loadingMoreHistory = true
      try {
        const res = await fetch(
          `/api/instances/${instanceId}/history?session_key=${encodeURIComponent(sessionKey)}&limit=50&before=${cursor}`,
          { headers: { Authorization: `Bearer ${auth.token}` } },
        )
        if (!res.ok) {
          toast.error(`Failed to load older messages (HTTP ${res.status})`)
          return
        }
        const data = await res.json()
        const older: ChatMessage[] = data.messages.map(storedToChat)
        this.messages[key] = [...older, ...existing]
        this.hasMoreHistory[key] = !!data.has_more
      } catch (e: any) {
        toast.error(`Can't load older messages: ${e?.message || e}`)
      } finally {
        this.loadingMoreHistory = false
      }
    },
  },
})
