import { ref } from 'vue'

export interface Toast {
  id: number
  kind: 'error' | 'info' | 'success'
  message: string
}

const toasts = ref<Toast[]>([])
let nextId = 1

function push(kind: Toast['kind'], message: string, ttlMs = 4000) {
  const id = nextId++
  toasts.value.push({ id, kind, message })
  setTimeout(() => {
    toasts.value = toasts.value.filter(t => t.id !== id)
  }, ttlMs)
}

export function useToast() {
  return {
    toasts,
    error: (msg: string) => push('error', msg, 6000),
    info: (msg: string) => push('info', msg),
    success: (msg: string) => push('success', msg),
    dismiss: (id: number) => {
      toasts.value = toasts.value.filter(t => t.id !== id)
    },
  }
}
