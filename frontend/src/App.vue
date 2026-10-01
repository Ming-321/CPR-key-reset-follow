<script setup lang="ts">
import type { Link, Sample, Snapshot } from './api/modules/follow'
import { BaseButton, BaseCheckbox, BaseIconButton, BaseModal, BaseSelect, BaseSwitch, BaseTag } from '@codex-proxy/ui'
import { ChevronRight, Plus, RefreshCw } from '@lucide/vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { command, load } from './api/modules/follow'
import Help from './Help.vue'

const data = ref<Snapshot>()
const busy = ref(false)
const error = ref('')
const modal = ref('')
const selected = ref('')
const keyId = ref('')
const accountId = ref('')
const preview = ref<Sample>()
const previewError = ref('')
const previewBusy = ref(false)
const acknowledge = ref(false)
const modalOpen = computed({ get: () => !!modal.value, set: (value) => {
  if (!value)
    modal.value = ''
} })
const rows = computed(() => Object.values(data.value?.state.links || {}))
const link = computed(() => data.value?.state.links[selected.value])
const account = (l: Link) => data.value?.accounts.find(a => a.account_id === l.account_id)
const observation = (l: Link) => data.value?.state.accounts[l.account_id]
const sample = (l: Link) => observation(l)?.sample || l.baseline
const budget = (l: Link) => data.value?.budgets[l.key_id]
const keyName = (l: Link) => data.value?.keys.find(k => k.id === l.key_id)?.name || 'Key 已删除'
const date = (value: number | null | undefined, full = false) => value == null ? '尚未开启或已到期' : full ? new Date(value).toLocaleString() : new Intl.DateTimeFormat('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(value)
function status(l: Link): '监测中' | '等待确认' | '需处理' | '已暂停' {
  if (l.paused)
    return '已暂停'
  if (l.pending?.kind === 'unknown' || l.pending?.kind === 'ready' || (observation(l)?.failure_since != null && Date.now() - observation(l)!.failure_since! >= 3600000))
    return '需处理'
  return ['confirm', 'changed'].includes(l.pending?.kind || '') ? '等待确认' : '监测中'
}
const tone = (l: Link) => ({ 监测中: 'success', 等待确认: 'warning', 需处理: 'danger', 已暂停: 'neutral' } as const)[status(l)]
const needs = computed(() => rows.value.filter(l => ['等待确认', '需处理'].includes(status(l))))
const keys = computed(() => (data.value?.keys || []).map(k => ({ value: k.id, label: k.name, disabled: !!data.value?.state.links[k.id], description: data.value?.state.links[k.id] ? `已关联 ${account(data.value.state.links[k.id])?.name || '账号'}` : undefined })))
const accounts = computed(() => (data.value?.accounts || []).map(a => ({ value: a.account_id, label: a.name || a.email || a.account_id, description: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') ? '仅支持 Codex 账号周窗口' : a.email || undefined, disabled: (a.provider_id !== 'openai' || a.authentication_kind !== 'oauth') })))
async function refresh() {
  try {
    data.value = await load()
    error.value = ''
  }
  catch (e) { error.value = (e as Error).message }
}
function open(kind: string, l?: Link) {
  selected.value = l?.key_id || ''
  modal.value = kind
  acknowledge.value = false
  error.value = ''
  if (kind === 'add') {
    keyId.value = ''
    accountId.value = ''
    preview.value = undefined
    previewError.value = ''
  }
}
async function act(action: string, extra: object = {}, close = true) {
  if (busy.value)
    return
  busy.value = true
  error.value = ''
  try {
    await command({ action, key_id: selected.value, version: data.value?.version, acknowledge: acknowledge.value, ...extra })
    if (close)
      modal.value = ''
    await refresh()
  }
  catch (e) { error.value = (e as Error).message }
  finally { busy.value = false }
}
let previewGeneration = 0
watch(accountId, async (id) => {
  const generation = ++previewGeneration
  preview.value = undefined
  previewError.value = ''
  if (!id)
    return
  previewBusy.value = true
  try {
    const result = await command({ action: 'preview', account_id: id })
    if (generation === previewGeneration)
      preview.value = result.sample
  }
  catch (e) {
    if (generation === previewGeneration)
      previewError.value = (e as Error).message
  }
  finally {
    if (generation === previewGeneration)
      previewBusy.value = false
  }
})
let timer: ReturnType<typeof setInterval>
onMounted(() => {
  refresh()
  timer = setInterval(() => {
    if (!document.hidden && !modal.value && !busy.value)
      refresh()
  }, 30000)
})
onUnmounted(() => clearInterval(timer))
</script>

<template>
  <main class="panel">
    <p v-if="error && !modal" role="alert" class="error">
      {{ error }} <BaseButton size="sm" @click="refresh">
        重试
      </BaseButton>
    </p>
    <div v-if="needs.length" class="banner">
      <span>有 {{ needs.length }} 个关联需要处理</span><BaseButton size="sm" @click="open('process', needs[0])">
        处理
      </BaseButton>
    </div>
    <div class="toolbar">
      <span class="muted">{{ rows.length }} 个关联</span><div class="tools">
        <label class="toggle"><BaseSwitch label="提前重置自动清零" :model-value="data?.state.early_auto ?? true" :disabled="busy || !data" @update:model-value="act('settings', { enabled: $event })" />提前重置自动清零</label><Help>开启时，确认账号提前进入新周周期后自动清零<br>关闭时等待你确认，正常换周仍自动清零</Help><BaseIconButton label="刷新" :disabled="busy" @click="refresh">
          <RefreshCw />
        </BaseIconButton><BaseButton variant="primary" :disabled="!data" @click="open('add')">
          <Plus :size="16" />添加关联
        </BaseButton>
      </div>
    </div>
    <p v-if="!data && !error" class="empty" role="status">
      正在加载关联
    </p>
    <div v-else-if="data && !rows.length" class="empty">
      <p>尚未建立关联</p><BaseButton variant="primary" @click="open('add')">
        添加关联
      </BaseButton>
    </div>
    <table v-else-if="data">
      <thead><tr><th>关联</th><th>账号周期</th><th>Key 周预算</th><th>状态</th><th><span class="sr-only">操作</span></th></tr></thead><tbody>
        <tr v-for="l in rows" :key="l.key_id">
          <td><strong>{{ keyName(l) }}</strong><small>{{ account(l)?.name || '账号已删除' }}<span class="email"> · {{ account(l)?.email }}</span></small></td>
          <td><span :title="date(sample(l).reset, true)">重置 {{ date(sample(l).reset) }}</span><small>账号已用 {{ sample(l).used == null ? '未知' : `${sample(l).used}%` }}</small><small v-if="observation(l)?.error">最近检查失败</small><small v-else-if="sample(l).reset <= Date.now()">上游窗口已到期，等待新窗口</small></td>
          <td v-if="budget(l)">
            <span>${{ budget(l)!.weekly_used_usd }} / {{ Number(budget(l)!.weekly_limit_usd) === 0 ? '不限' : `$${budget(l)!.weekly_limit_usd}` }}</span><progress v-if="Number(budget(l)!.weekly_limit_usd) > 0" :value="Number(budget(l)!.weekly_used_usd)" :max="Number(budget(l)!.weekly_limit_usd)" /><small class="expiry">{{ budget(l)!.weekly_resets_at_ms ? `原生到期 ${date(budget(l)!.weekly_resets_at_ms)}` : '尚未开启或已到期' }}<Help v-if="budget(l)!.weekly_resets_at_ms && budget(l)!.weekly_resets_at_ms! < sample(l).reset" label="原生到期说明">Key 的原生周窗口会比账号窗口先到期并自行清零一次，本插件只在确认账号新周期后清零</Help></small>
          </td>
          <td v-else>
            <small>无法读取 Key 预算</small>
          </td>
          <td>
            <div class="status">
              <BaseTag :type="tone(l)" size="sm">
                {{ status(l) }}
              </BaseTag><BaseButton v-if="['等待确认', '需处理'].includes(status(l))" size="sm" @click="open('process', l)">
                处理
              </BaseButton>
            </div>
          </td>
          <td>
            <div class="tools">
              <BaseIconButton label="立即检查" title="立即检查" :disabled="busy || l.paused" @click="selected = l.key_id; act('check')">
                <RefreshCw />
              </BaseIconButton><BaseIconButton label="查看详情" @click="open('detail', l)">
                <ChevronRight />
              </BaseIconButton>
            </div>
          </td>
        </tr>
      </tbody>
    </table>
    <footer>仅决定周预算何时清零，不改变请求路由<Help>仅在确认账号新周期后清零，不改变 Key 的原生到期时间<br>不修改预算上限，不创建 Key，也不控制请求路由</Help></footer>
  </main>
  <BaseModal v-model="modalOpen" :title="({ add: '添加关联', detail: '关联详情', process: '处理关联', delete: '删除关联' })[modal] || '关联'" :dismissible="!busy" :draggable="false">
    <p v-if="error" role="alert" class="error">
      {{ error }}
    </p>
    <template v-if="modal === 'add'">
      <label class="field">Key<BaseSelect v-model="keyId" :options="keys" filterable placeholder="选择现有 Key" :disabled="busy" /></label>
      <p v-if="keyId && Number(data?.budgets[keyId]?.weekly_limit_usd) === 0" class="muted">
        该 Key 未设周限额，清零无实际作用
      </p>
      <label class="field">额度来源账号<BaseSelect v-model="accountId" :options="accounts" filterable placeholder="选择账号名称或邮箱" :disabled="busy" /></label>
      <p v-if="previewBusy" role="status">
        正在识别周窗口
      </p><p v-else-if="previewError" class="error">
        {{ previewError }}
      </p><p v-else-if="preview">
        已识别 Codex 周窗口，上游预计重置 {{ date(preview.reset) }}
      </p>
    </template>
    <template v-else-if="link && modal === 'detail'">
      <h3>{{ keyName(link) }}</h3><h4>账号周期</h4><p>{{ account(link)?.name }} · {{ account(link)?.email }}</p><p>预计重置 {{ date(sample(link).reset, true) }} · 已用 {{ sample(link).used ?? '未知' }}%</p>
      <h4>Key 预算</h4><p>${{ budget(link)?.weekly_used_usd }} / ${{ budget(link)?.weekly_limit_usd }}</p><p>原生到期 {{ date(budget(link)?.weekly_resets_at_ms, true) }}</p>
      <h4>最近事件</h4><p v-for="(event, index) in [...link.events].reverse()" :key="index" class="muted">
        {{ date(event.at) }} · {{ event.reason }}
      </p>
      <details>
        <summary>判定与诊断</summary><p>窗口 codex:604800s · 604800 秒</p><p>已确认周期结束 {{ date(link.baseline.reset, true) }}</p><p>最近样本 {{ date(sample(link).observed, true) }}</p><p v-if="link.pending">
          待处理原因 {{ link.pending.reason }}
        </p><p v-if="link.pending?.before">
          重置前周用量 ${{ link.pending.before.weekly_used_usd }}
        </p><p>{{ link.pending?.error || observation(link)?.error }}</p>
      </details>
      <h4>操作</h4><div class="tools">
        <BaseButton :disabled="busy" @click="act(link.paused ? 'resume' : 'pause')">
          {{ link.paused ? '恢复并建立新基线' : '暂停关联' }}
        </BaseButton><BaseButton variant="destructive" @click="modal = 'delete'; acknowledge = false">
          删除关联
        </BaseButton>
      </div>
    </template>
    <template v-else-if="link && modal === 'process'">
      <template v-if="link.pending?.kind === 'unknown'">
        <p>重置结果未知，已停止自动重试</p><p class="muted">
          保留现有用量并跳过会重新建立基线
        </p><BaseCheckbox v-model="acknowledge" label="我了解可能清掉期间新产生的消费" />
      </template>
      <template v-else-if="link.pending?.kind === 'changed'">
        <p>目标窗口标识或时长发生变化，已停止自动清零</p>
        <p v-for="window in link.pending.windows" :key="window.key">
          {{ window.key }} · {{ window.window_seconds }} 秒 · {{ date(window.reset_at_ms, true) }}
        </p>
        <p class="muted">
          请在宿主检查账号套餐，恢复唯一 Codex 周窗口后再确认或更新基线
        </p>
      </template>
      <template v-else-if="link.pending?.kind === 'confirm'">
        <p>{{ link.pending.reason === 'early' ? '账号提前进入新周期，自动清零开关已关闭' : '周期边界变化无法解释，需要你确认' }}</p><dl><dt>原边界</dt><dd>{{ date(link.baseline.reset, true) }}</dd><dt>新边界</dt><dd>{{ date(link.pending.sample.reset, true) }}</dd><dt>推算的新窗口起点</dt><dd>{{ date(link.pending.sample.reset - 604800000, true) }}</dd></dl><p class="muted">
          确认后会清零当前周用量，此操作无法撤销
        </p>
      </template>
      <p v-else>
        {{ observation(link)?.error || '读取预算失败，尚未调用清零' }}
      </p>
    </template>
    <template v-else-if="modal === 'delete'">
      <p>仅删除本插件的关联记录，不撤销已执行的重置</p><BaseCheckbox v-model="acknowledge" label="确认删除此关联" />
    </template>
    <template #footer>
      <span v-if="modal === 'add'" class="muted">仅建立基线，不清零</span><BaseButton :disabled="busy" @click="modal = ''">
        {{ modal === 'detail' ? '关闭' : '取消' }}
      </BaseButton>
      <BaseButton v-if="modal === 'add'" variant="primary" :loading="busy" :disabled="!keyId || !preview || previewBusy" @click="act('add', { key_id: keyId, account_id: accountId })">
        添加
      </BaseButton>
      <template v-if="modal === 'process' && link?.pending">
        <BaseButton :disabled="busy" @click="act('skip')">
          {{ link.pending.kind === 'unknown' ? '保留现有用量并跳过' : '忽略并更新基线' }}
        </BaseButton>
        <BaseButton v-if="link.pending.kind === 'unknown'" variant="destructive" :disabled="!acknowledge || busy" @click="act('retry')">
          再清零一次
        </BaseButton>
        <BaseButton v-if="['confirm', 'changed'].includes(link.pending.kind)" variant="primary" :loading="busy" @click="act('confirm')">
          确认并清零
        </BaseButton>
      </template>
      <BaseButton v-if="modal === 'process' && !link?.pending" :loading="busy" @click="act('check')">
        立即检查
      </BaseButton>
      <BaseButton v-if="modal === 'delete'" variant="destructive" :disabled="!acknowledge || busy" @click="act('delete')">
        删除关联
      </BaseButton>
    </template>
  </BaseModal>
</template>
