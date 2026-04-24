<template>
  <div class="dashboard" @keydown="handleGlobalKeys">
    <!-- Sidebar -->
    <aside class="sidebar">
      <div class="sidebar-header">
        <h2>AgentPush</h2>
        <span :class="['status-dot', gw.wsConnected ? 'online' : 'offline']"
              :title="gw.wsConnected ? 'Connected' : 'Disconnected - retrying...'"></span>
      </div>
      <div v-if="!gw.wsConnected" class="ws-error">
        WebSocket disconnected, reconnecting...
      </div>

      <!-- G: Session search -->
      <div class="sidebar-search">
        <input
          v-model="searchQuery"
          placeholder="Search sessions..."
          class="search-input"
        />
      </div>

      <div class="instance-tree">
        <div v-for="inst in gw.instances" :key="inst.instance_id" class="instance-group">
          <div class="instance-header" @click="toggleInstance(inst.instance_id)">
            <span>{{ expanded[inst.instance_id] ? '&#x25BC;' : '&#x25B6;' }}</span>
            <span :class="['status-dot', inst.online ? 'online' : 'offline']"></span>
            {{ inst.instance_name }}
          </div>

          <div v-if="expanded[inst.instance_id]" class="projects">
            <div v-for="proj in inst.projects" :key="proj.name" class="project-group">
              <div class="project-name">{{ proj.name }}</div>
              <div
                v-for="sess in filteredSessions(sortedSessions(proj.sessions), inst.instance_id)"
                :key="sess.session_key"
                :class="['session-item', {
                  active: gw.activeInstance === inst.instance_id && gw.activeSession === sess.session_key,
                  pinned: isPinned(inst.instance_id, sess.session_key),
                }]"
                @click="selectAndMarkRead(inst.instance_id, sess.session_key)"
              >
                <!-- I: Pin toggle -->
                <span
                  class="pin-toggle"
                  @click.stop="togglePin(inst.instance_id, sess.session_key)"
                  :title="isPinned(inst.instance_id, sess.session_key) ? 'Unpin' : 'Pin to top'"
                >{{ isPinned(inst.instance_id, sess.session_key) ? '&#x2605;' : '&#x2606;' }}</span>
                <!-- A: Busy pulse in sidebar -->
                <span :class="sess.is_busy ? 'busy pulse' : 'idle'">&#x25CF;</span>
                <span class="session-label">{{ formatSessionName(sess) }}</span>
                <!-- C: Unread dot -->
                <span v-if="hasUnread(inst.instance_id, sess.session_key)" class="unread-dot"></span>
                <span class="session-age">{{ relativeTime(sess.updated_at) }}</span>
              </div>
            </div>
          </div>
        </div>

        <p v-if="gw.loadingInstances && gw.instances.length === 0" class="empty">
          Loading instances...
        </p>
        <div v-else-if="gw.instances.length === 0" class="empty-state">
          <p class="empty">No instances connected</p>
          <p class="empty-hint">
            Start one with:<br>
            <code>agentbridge run --gateway ws://localhost:9900</code>
          </p>
        </div>
      </div>

      <button class="logout-btn" @click="logout">Logout</button>
    </aside>

    <!-- Chat panel -->
    <main class="chat-panel">
      <div v-if="!gw.activeSession" class="no-session">
        <p>Select a session from the sidebar</p>
        <p class="shortcut-hint">Ctrl+K to focus search</p>
      </div>

      <template v-else>
        <!-- J+K: Chat header with status + work_dir -->
        <div class="chat-header">
          <div class="header-info">
            <div class="header-title">
              {{ activeSessionLabel }}
              <span v-if="activeSessionBusy" class="typing-indicator">Claude is thinking<span class="dots"><span>.</span><span>.</span><span>.</span></span></span>
            </div>
            <div v-if="activeWorkDir" class="header-workdir" :title="activeWorkDir">{{ activeWorkDir }}</div>
          </div>
          <div class="toolbar">
            <button @click="renameSession" title="Rename session (Ctrl+R)">&#x270F;&#xFE0F;</button>
            <button @click="gw.sendCommand('stop')" title="Stop">&#x1F6D1;</button>
            <button @click="confirmNewSession" title="New session">&#x1F195;</button>
            <button @click="gw.sendCommand('compress')" title="Compress">&#x1F5DC;&#xFE0F;</button>
          </div>
        </div>

        <div class="messages" ref="messagesRef">
          <div v-if="gw.loadingHistory && gw.activeMessages.length === 0" class="loading-history">
            Loading history...
          </div>
          <button
            v-if="canLoadMore"
            class="load-more-btn"
            :disabled="gw.loadingMoreHistory"
            @click="loadOlder"
          >
            {{ gw.loadingMoreHistory ? 'Loading...' : 'Load older messages' }}
          </button>
          <template v-for="(msg, idx) in gw.activeMessages" :key="msg.id">
            <!-- B: Time separator -->
            <div v-if="shouldShowTimeSeparator(idx)" class="time-separator">
              {{ formatTimeSeparator(msg.timestamp) }}
            </div>
            <div :class="['message', msg.type]" @mouseenter="hoveredMsg = msg.id" @mouseleave="hoveredMsg = ''">
              <!-- H: Copy button on hover -->
              <button
                v-if="hoveredMsg === msg.id && msg.type !== 'tool'"
                class="copy-btn"
                @click="copyMessage(msg.content)"
                title="Copy"
              >&#x1F4CB;</button>
              <template v-if="msg.type === 'tool' && msg.toolName">
                <div class="tool-card" @click="toggleTool(msg.id)">
                  <div class="tool-header">
                    <span class="tool-chevron">{{ openTools[msg.id] ? '&#x25BC;' : '&#x25B6;' }}</span>
                    <span class="tool-status">
                      {{ msg.toolHasResult ? (msg.toolIsError ? '&#x1F4A5;' : '&#x2713;') : '&#x23F3;' }}
                    </span>
                    <span class="tool-name">{{ msg.toolName }}</span>
                    <span class="tool-summary">{{ toolSummary(msg) }}</span>
                    <!-- H: Copy for tool -->
                    <button
                      v-if="hoveredMsg === msg.id"
                      class="copy-btn-inline"
                      @click.stop="copyMessage(msg.toolInput + (msg.toolOutput ? '\n---\n' + msg.toolOutput : ''))"
                      title="Copy tool I/O"
                    >&#x1F4CB;</button>
                  </div>
                  <div v-if="openTools[msg.id]" class="tool-body">
                    <div class="tool-section">
                      <div class="tool-section-label">input</div>
                      <pre class="tool-pre">{{ msg.toolInput }}</pre>
                    </div>
                    <div v-if="msg.toolHasResult" class="tool-section">
                      <div class="tool-section-label">
                        {{ msg.toolIsError ? 'error' : 'output' }}
                      </div>
                      <pre :class="['tool-pre', msg.toolIsError && 'error']">{{ msg.toolOutput }}</pre>
                    </div>
                    <div v-else class="tool-section">
                      <div class="tool-section-label">output</div>
                      <div class="tool-waiting">Running...</div>
                    </div>
                  </div>
                </div>
              </template>
              <template v-else>
                <div class="msg-content" v-html="formatMessage(msg.content)"></div>
                <!-- B: Timestamp on each message -->
                <div class="msg-time">{{ formatMsgTime(msg.timestamp) }}</div>
              </template>
            </div>
          </template>
        </div>

        <form class="input-bar" @submit.prevent="send">
          <input
            ref="inputRef"
            v-model="inputText"
            placeholder="Type a message... (Ctrl+K to focus)"
            @keydown="handleInputKeys"
          />
          <button type="submit" :disabled="!inputText.trim()">Send</button>
        </form>
      </template>
    </main>
  </div>
</template>

<script setup lang="ts">
import { useAuthStore } from '~/stores/auth'
import { useGatewayStore } from '~/stores/gateway'
import { useToast } from '~/composables/useToast'

const auth = useAuthStore()
const gw = useGatewayStore()
const toast = useToast()
const router = useRouter()
const inputText = ref('')
const inputRef = ref<HTMLInputElement | null>(null)
const messagesRef = ref<HTMLElement | null>(null)
const expanded = ref<Record<string, boolean>>({})
const openTools = ref<Record<string, boolean>>({})
const searchQuery = ref('')
const hoveredMsg = ref('')

// I: Pinned sessions (localStorage)
const pinnedSessions = ref<Record<string, boolean>>(
  JSON.parse(localStorage.getItem('agentbridge_pins') || '{}')
)

// C: Last-read timestamps per session (localStorage)
const lastRead = ref<Record<string, number>>(
  JSON.parse(localStorage.getItem('agentbridge_lastread') || '{}')
)

// Auth guard
if (!auth.isAuthenticated) {
  router.push('/login')
} else {
  gw.connect()
}

// --- Instance & session helpers ---

function toggleInstance(id: string) {
  expanded.value[id] = !expanded.value[id]
}

function toggleTool(id: string) {
  openTools.value[id] = !openTools.value[id]
}

// I: Pin / unpin
function pinKey(instanceId: string, sessionKey: string) {
  return `${instanceId}::${sessionKey}`
}

function isPinned(instanceId: string, sessionKey: string): boolean {
  return !!pinnedSessions.value[pinKey(instanceId, sessionKey)]
}

function togglePin(instanceId: string, sessionKey: string) {
  const k = pinKey(instanceId, sessionKey)
  if (pinnedSessions.value[k]) {
    delete pinnedSessions.value[k]
  } else {
    pinnedSessions.value[k] = true
  }
  localStorage.setItem('agentbridge_pins', JSON.stringify(pinnedSessions.value))
}

function sortedSessions(sessions: any[]): any[] {
  return [...sessions].sort((a, b) =>
    new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
  )
}

// G: Filter sessions by search query; I: pinned first
function filteredSessions(sessions: any[], instanceId: string): any[] {
  let list = sessions
  if (searchQuery.value.trim()) {
    const q = searchQuery.value.toLowerCase()
    list = list.filter((s: any) => {
      const name = (s.name || s.session_key || '').toLowerCase()
      return name.includes(q)
    })
  }
  // Pinned sessions bubble to top
  return list.sort((a: any, b: any) => {
    const ap = isPinned(instanceId, a.session_key) ? 1 : 0
    const bp = isPinned(instanceId, b.session_key) ? 1 : 0
    return bp - ap
  })
}

// C: Unread detection
function hasUnread(instanceId: string, sessionKey: string): boolean {
  if (gw.activeInstance === instanceId && gw.activeSession === sessionKey) return false
  const key = `${instanceId}:${sessionKey}`
  const msgs = gw.messages[key]
  if (!msgs || msgs.length === 0) return false
  const lastTs = msgs[msgs.length - 1].timestamp || 0
  const readTs = lastRead.value[key] || 0
  return lastTs > readTs
}

function selectAndMarkRead(instanceId: string, sessionKey: string) {
  gw.selectSession(instanceId, sessionKey)
  markRead(instanceId, sessionKey)
}

function markRead(instanceId: string, sessionKey: string) {
  const key = `${instanceId}:${sessionKey}`
  lastRead.value[key] = Date.now()
  localStorage.setItem('agentbridge_lastread', JSON.stringify(lastRead.value))
}

// Mark active session as read whenever new messages arrive
watch(() => gw.activeMessages.length, () => {
  if (gw.activeInstance && gw.activeSession) {
    markRead(gw.activeInstance, gw.activeSession)
  }
})

// --- Chat header computed ---

const canLoadMore = computed(() => {
  if (!gw.activeInstance || !gw.activeSession) return false
  const key = `${gw.activeInstance}:${gw.activeSession}`
  return gw.hasMoreHistory[key] === true
})

function loadOlder() {
  if (!gw.activeInstance || !gw.activeSession) return
  gw.loadOlderHistory(gw.activeInstance, gw.activeSession)
}

const activeSessionLabel = computed(() => {
  if (!gw.activeInstance || !gw.activeSession) return ''
  const inst = gw.instances.find((i: any) => i.instance_id === gw.activeInstance)
  if (!inst) return gw.activeSession
  for (const proj of inst.projects) {
    const sess = proj.sessions.find((s: any) => s.session_key === gw.activeSession)
    if (sess) return formatSessionName(sess) || gw.activeSession
  }
  return gw.activeSession
})

// A: Is the active session busy?
const activeSessionBusy = computed(() => {
  if (!gw.activeInstance || !gw.activeSession) return false
  const inst = gw.instances.find((i: any) => i.instance_id === gw.activeInstance)
  if (!inst) return false
  for (const proj of inst.projects) {
    const sess = proj.sessions.find((s: any) => s.session_key === gw.activeSession)
    if (sess) return sess.is_busy
  }
  return false
})

// K+J: Working directory for active session
const activeWorkDir = computed(() => {
  if (!gw.activeInstance) return ''
  const inst = gw.instances.find((i: any) => i.instance_id === gw.activeInstance)
  if (!inst) return ''
  for (const proj of inst.projects) {
    if (proj.sessions.some((s: any) => s.session_key === gw.activeSession)) {
      return proj.work_dir
    }
  }
  return ''
})

// E: Confirm before /new
function confirmNewSession() {
  if (confirm('Create a new session? The current session will be paused.')) {
    gw.sendCommand('new')
  }
}

function renameSession() {
  const name = prompt('New session name:')
  if (name && name.trim()) {
    gw.sendCommand(`name ${name.trim()}`)
  }
}

// --- Message formatting ---

function toolSummary(msg: any): string {
  const input = (msg.toolInput || '').trim()
  const firstLine = input.split('\n')[0]
  return firstLine.length > 80 ? firstLine.slice(0, 77) + '...' : firstLine
}

function formatSessionName(sess: any): string {
  const parts = sess.session_key.split(':')
  const platform = parts[0] || 'unknown'
  const icon = platform === 'discord' ? '\u{1F4AC}' : platform === 'telegram' ? '\u{1F4F1}' : '\u{1F517}'
  if (sess.name) return `${icon} ${sess.name}`
  const id = parts[1] || ''
  const shortId = id.length > 8 ? id.slice(-6) : id
  return `${icon} ${platform} #${shortId}`
}

function relativeTime(dateStr: string): string {
  const now = Date.now()
  const then = new Date(dateStr).getTime()
  const diffMs = now - then
  if (isNaN(diffMs) || diffMs < 0) return ''
  const mins = Math.floor(diffMs / 60000)
  if (mins < 1) return 'now'
  if (mins < 60) return `${mins}m`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h`
  const days = Math.floor(hours / 24)
  return `${days}d`
}

// B: Time separator logic
function shouldShowTimeSeparator(idx: number): boolean {
  if (idx === 0) return true
  const msgs = gw.activeMessages
  const curr = msgs[idx].timestamp || 0
  const prev = msgs[idx - 1].timestamp || 0
  return curr - prev > 10 * 60 * 1000
}

function formatTimeSeparator(ts: number): string {
  if (!ts) return ''
  const d = new Date(ts)
  const now = new Date()
  const isToday = d.toDateString() === now.toDateString()
  const yesterday = new Date(now)
  yesterday.setDate(yesterday.getDate() - 1)
  const isYesterday = d.toDateString() === yesterday.toDateString()
  const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  if (isToday) return `Today ${time}`
  if (isYesterday) return `Yesterday ${time}`
  return `${d.toLocaleDateString()} ${time}`
}

// B: Per-message timestamp
function formatMsgTime(ts: number): string {
  if (!ts) return ''
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

// F: Format message with URL linkify + markdown
function formatMessage(content: string): string {
  return content
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/```(\w*)\n([\s\S]*?)```/g, '<pre><code>$2</code></pre>')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" target="_blank" class="link">$1</a>')
    .replace(/(^|[^"=])(https?:\/\/[^\s<]+)/g, '$1<a href="$2" target="_blank" class="link">$2</a>')
    .replace(/\n/g, '<br>')
}

// H: Copy to clipboard
function copyMessage(text: string) {
  navigator.clipboard.writeText(text).then(() => {
    toast.success('Copied')
  }).catch(() => {
    toast.error('Failed to copy')
  })
}

// --- Keyboard shortcuts ---

// D: Global shortcuts
function handleGlobalKeys(e: KeyboardEvent) {
  const mod = e.metaKey || e.ctrlKey
  if (mod && e.key === 'k') {
    e.preventDefault()
    inputRef.value?.focus()
  }
}

// D: Input-specific shortcuts
function handleInputKeys(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    inputText.value = ''
    inputRef.value?.blur()
  }
}

// --- Actions ---

function send() {
  const text = inputText.value.trim()
  if (!text) return
  gw.sendMessage(text)
  inputText.value = ''
}

function logout() {
  auth.logout()
  router.push('/login')
}

// Auto-expand first instance
watch(() => gw.instances, (instances) => {
  for (const inst of instances) {
    if (!(inst.instance_id in expanded.value)) {
      expanded.value[inst.instance_id] = true
    }
  }
}, { immediate: true })

// Auto-scroll on new messages
watch(() => gw.activeMessages.length, () => {
  nextTick(() => {
    if (messagesRef.value) {
      messagesRef.value.scrollTop = messagesRef.value.scrollHeight
    }
  })
})
</script>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: #1a1a2e; color: #e0e0e0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; }

.dashboard { display: flex; height: 100vh; outline: none; }

/* Sidebar */
.sidebar {
  width: 280px;
  background: #16213e;
  border-right: 1px solid #2a2a4a;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}
.sidebar-header {
  padding: 1rem;
  border-bottom: 1px solid #2a2a4a;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
.sidebar-header h2 { font-size: 1.1rem; }

.status-dot {
  width: 8px; height: 8px; border-radius: 50%; display: inline-block;
}
.status-dot.online { background: #53c28b; }
.status-dot.offline { background: #e74c3c; }

/* G: Search */
.sidebar-search {
  padding: 0.4rem 0.5rem;
  border-bottom: 1px solid #2a2a4a;
}
.search-input {
  width: 100%;
  padding: 0.35rem 0.6rem;
  background: #0d1b2a;
  border: 1px solid #2a2a4a;
  border-radius: 6px;
  color: #ccc;
  font-size: 0.8rem;
  outline: none;
}
.search-input:focus { border-color: #53c28b; }
.search-input::placeholder { color: #555; }

.instance-tree { flex: 1; padding: 0.5rem; }
.instance-header {
  padding: 0.4rem 0.5rem;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-weight: 600;
  font-size: 0.9rem;
}
.instance-header:hover { background: #1a2744; border-radius: 4px; }

.project-name {
  padding: 0.2rem 0.5rem 0.2rem 1.5rem;
  font-size: 0.8rem;
  color: #888;
}

.session-item {
  padding: 0.3rem 0.5rem 0.3rem 1.8rem;
  cursor: pointer;
  font-size: 0.85rem;
  border-radius: 4px;
  display: flex;
  align-items: center;
  gap: 0.3rem;
  position: relative;
}
.session-item:hover { background: #1a2744; }
.session-item.active { background: #0f3460; }
.session-item.pinned { border-left: 2px solid #53c28b; }
.session-item .busy { color: #53c28b; font-size: 0.6rem; }
.session-item .idle { color: #666; font-size: 0.6rem; }

/* A: Busy pulse animation */
.session-item .pulse {
  animation: pulse 1.5s ease-in-out infinite;
}
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

/* I: Pin toggle */
.pin-toggle {
  font-size: 0.7rem;
  color: #555;
  cursor: pointer;
  flex-shrink: 0;
}
.pin-toggle:hover { color: #53c28b; }
.session-item.pinned .pin-toggle { color: #53c28b; }

.session-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* C: Unread dot */
.unread-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #6ea8ff;
  flex-shrink: 0;
}

.session-age {
  color: #555;
  font-size: 0.7rem;
  flex-shrink: 0;
  margin-left: auto;
}

.ws-error {
  padding: 0.4rem 0.8rem;
  background: #3d1212;
  color: #f87171;
  font-size: 0.75rem;
  text-align: center;
}

.empty { padding: 1rem; color: #666; text-align: center; font-size: 0.85rem; }
.empty-state { padding: 0.5rem; }
.empty-hint {
  color: #666;
  font-size: 0.75rem;
  text-align: center;
  margin-top: 0.5rem;
  line-height: 1.6;
}
.empty-hint code {
  display: inline-block;
  margin-top: 0.3rem;
  padding: 0.3rem 0.5rem;
  background: #0d1b2a;
  border-radius: 4px;
  font-size: 0.7rem;
  word-break: break-all;
}
.loading-history {
  text-align: center;
  padding: 2rem;
  color: #666;
  font-size: 0.9rem;
}
.load-more-btn {
  display: block;
  margin: 0.5rem auto 1rem;
  padding: 0.4rem 1rem;
  background: transparent;
  border: 1px solid #2a4565;
  color: #6ea8ff;
  border-radius: 16px;
  cursor: pointer;
  font-size: 0.8rem;
}
.load-more-btn:hover:not(:disabled) { border-color: #6ea8ff; }
.load-more-btn:disabled { opacity: 0.5; cursor: not-allowed; }

.logout-btn {
  margin: 0.5rem;
  padding: 0.5rem;
  background: transparent;
  border: 1px solid #333;
  color: #888;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.8rem;
}
.logout-btn:hover { border-color: #e74c3c; color: #e74c3c; }

.shortcut-hint { color: #444; font-size: 0.75rem; margin-top: 0.5rem; }

/* Chat panel */
.chat-panel { flex: 1; display: flex; flex-direction: column; }

.no-session {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: #666;
}

/* J+K: Header with status line */
.chat-header {
  padding: 0.6rem 1rem;
  border-bottom: 1px solid #2a2a4a;
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.header-info { display: flex; flex-direction: column; gap: 0.15rem; min-width: 0; }
.header-title {
  font-size: 0.9rem;
  color: #ccc;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
.header-workdir {
  font-size: 0.7rem;
  color: #555;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* A: Typing indicator */
.typing-indicator {
  font-size: 0.75rem;
  color: #53c28b;
  font-style: italic;
}
.dots span {
  animation: dotPulse 1.4s infinite;
}
.dots span:nth-child(2) { animation-delay: 0.2s; }
.dots span:nth-child(3) { animation-delay: 0.4s; }
@keyframes dotPulse {
  0%, 80%, 100% { opacity: 0; }
  40% { opacity: 1; }
}

.toolbar { display: flex; gap: 0.3rem; flex-shrink: 0; }
.toolbar button {
  background: transparent;
  border: 1px solid #333;
  color: #ccc;
  padding: 0.3rem 0.5rem;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.85rem;
}
.toolbar button:hover { border-color: #53c28b; }

.messages {
  flex: 1;
  overflow-y: auto;
  padding: 1rem;
}

/* B: Time separator */
.time-separator {
  text-align: center;
  color: #555;
  font-size: 0.7rem;
  margin: 1rem 0 0.5rem;
  position: relative;
}
.time-separator::before,
.time-separator::after {
  content: '';
  position: absolute;
  top: 50%;
  width: calc(50% - 60px);
  height: 1px;
  background: #2a2a4a;
}
.time-separator::before { left: 0; }
.time-separator::after { right: 0; }

.message {
  margin-bottom: 0.75rem;
  max-width: 85%;
  position: relative;
}
.message.user {
  margin-left: auto;
  background: #0f3460;
  padding: 0.6rem 1rem;
  border-radius: 12px 12px 4px 12px;
}
.message.assistant {
  background: #1e2a45;
  padding: 0.6rem 1rem;
  border-radius: 12px 12px 12px 4px;
}
.message.system {
  text-align: center;
  color: #666;
  font-size: 0.8rem;
  max-width: 100%;
}
.message.tool {
  color: #888;
  font-size: 0.85rem;
  max-width: 100%;
}

/* B: Per-message timestamp */
.msg-time {
  font-size: 0.65rem;
  color: #444;
  margin-top: 0.2rem;
  text-align: right;
}
.message.user .msg-time { text-align: right; }
.message.assistant .msg-time { text-align: left; }

/* H: Copy button */
.copy-btn {
  position: absolute;
  top: 0.3rem;
  right: 0.3rem;
  background: #1a2744;
  border: 1px solid #2a4565;
  border-radius: 4px;
  padding: 0.15rem 0.3rem;
  font-size: 0.7rem;
  cursor: pointer;
  color: #888;
  z-index: 5;
}
.copy-btn:hover { color: #ccc; border-color: #53c28b; }
.copy-btn-inline {
  background: transparent;
  border: none;
  font-size: 0.7rem;
  cursor: pointer;
  color: #666;
  margin-left: auto;
  flex-shrink: 0;
}
.copy-btn-inline:hover { color: #ccc; }

.tool-card {
  background: #12223a;
  border: 1px solid #22365a;
  border-radius: 6px;
  overflow: hidden;
}
.tool-header {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.4rem 0.6rem;
  cursor: pointer;
  user-select: none;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.8rem;
}
.tool-header:hover { background: #162c4a; }
.tool-chevron { color: #666; width: 0.8rem; }
.tool-status { width: 1rem; text-align: center; }
.tool-name { color: #6ea8ff; font-weight: 600; }
.tool-summary {
  color: #888;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}
.tool-body {
  border-top: 1px solid #22365a;
  padding: 0.5rem 0.6rem;
}
.tool-section { margin-bottom: 0.5rem; }
.tool-section:last-child { margin-bottom: 0; }
.tool-section-label {
  color: #666;
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 0.2rem;
}
.tool-pre {
  background: #0d1b2a;
  padding: 0.5rem;
  border-radius: 4px;
  font-size: 0.8rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 300px;
  overflow-y: auto;
  color: #ccc;
  position: relative;
}
.tool-pre.error { color: #f87171; }
.tool-waiting {
  color: #666;
  font-style: italic;
  font-size: 0.8rem;
}

.msg-content { line-height: 1.5; word-break: break-word; }
.msg-content code {
  background: #0d1b2a;
  padding: 0.15rem 0.4rem;
  border-radius: 3px;
  font-size: 0.85em;
}
.msg-content pre {
  background: #0d1b2a;
  padding: 0.75rem;
  border-radius: 6px;
  margin: 0.5rem 0;
  overflow-x: auto;
  position: relative;
}
.msg-content pre code { padding: 0; background: none; }

/* F: Link styling */
.msg-content .link {
  color: #6ea8ff;
  text-decoration: underline;
  text-underline-offset: 2px;
}
.msg-content .link:hover { color: #93c5fd; }

/* Input bar */
.input-bar {
  padding: 0.75rem 1rem;
  border-top: 1px solid #2a2a4a;
  display: flex;
  gap: 0.5rem;
}
.input-bar input {
  flex: 1;
  padding: 0.6rem 1rem;
  background: #0f3460;
  border: 1px solid #333;
  border-radius: 8px;
  color: #fff;
  font-size: 0.95rem;
  outline: none;
}
.input-bar input:focus { border-color: #53c28b; }
.input-bar button {
  padding: 0.6rem 1.2rem;
  background: #53c28b;
  border: none;
  border-radius: 8px;
  color: #fff;
  font-weight: 600;
  cursor: pointer;
}
.input-bar button:hover { background: #45a876; }
.input-bar button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
