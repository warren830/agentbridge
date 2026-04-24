<template>
  <div class="toast-stack">
    <div
      v-for="t in toasts"
      :key="t.id"
      :class="['toast', t.kind]"
      @click="dismiss(t.id)"
    >
      <span>{{ iconFor(t.kind) }} {{ t.message }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useToast } from '~/composables/useToast'

const { toasts, dismiss } = useToast()

function iconFor(kind: string): string {
  return kind === 'error' ? '⚠️' : kind === 'success' ? '✓' : 'ℹ'
}
</script>

<style scoped>
.toast-stack {
  position: fixed;
  top: 1rem;
  right: 1rem;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  max-width: 360px;
  pointer-events: none;
}

.toast {
  padding: 0.75rem 1rem;
  border-radius: 8px;
  font-size: 0.9rem;
  color: #fff;
  cursor: pointer;
  pointer-events: auto;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  animation: slideIn 0.2s ease-out;
}

.toast.error { background: #dc2626; }
.toast.success { background: #16a34a; }
.toast.info { background: #2563eb; }

@keyframes slideIn {
  from { transform: translateX(100%); opacity: 0; }
  to { transform: translateX(0); opacity: 1; }
}
</style>
