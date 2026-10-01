<script setup lang="ts">
import { BaseIconButton, BasePopover } from '@codex-proxy/ui'
import { Info } from '@lucide/vue'

defineProps<{ label?: string }>()
</script>

<template>
  <BasePopover placement="bottom-end">
    <template #trigger>
      <BaseIconButton :label="label || '查看说明'" size="sm">
        <Info :size="14" />
      </BaseIconButton>
    </template>
    <div class="help">
      <slot />
    </div>
  </BasePopover>
</template>
