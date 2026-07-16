export type LocalLanguage = 'en' | 'zh-CN'

export const LANGUAGE_OPTIONS = [
  { value: 'en', label: 'English', meta: 'EN' },
  { value: 'zh-CN', label: '简体中文', meta: '中' },
] as const

const english = {
  loading: 'Starting local workspace…',
  brand: 'cocli local',
  serviceOnline: 'SQLite + HTTP online',
  dismiss: 'Dismiss',
  channelsAndRuntimes: 'Channels and runtimes',
  channels: 'Channels',
  channelsNav: 'Channels',
  noChannels: 'Create a channel to open your first local workspace.',
  newChannel: 'New channel',
  channelPlaceholder: 'product-loop',
  add: 'Add',
  runtimes: 'Local runtimes',
  installed: 'installed',
  unavailable: 'unavailable',
  noRuntimes: 'No runtime catalog is configured.',
  conversation: 'Channel conversation',
  localChannel: 'LOCAL CHANNEL',
  chooseChannel: 'Choose a channel',
  agentCount: '{{count}} agent',
  agentCountPlural: '{{count}} agents',
  createChannel: 'Create a channel',
  createChannelDescription: 'Your channels, agents, and message history stay in the local SQLite database.',
  loadingMessages: 'Loading message history…',
  startTask: 'Start a real task',
  startTaskDescription: 'Add an installed runtime agent, then send a concrete prompt from the composer below.',
  you: 'you',
  agent: 'agent',
  taskFor: 'Task for #{{channel}}',
  selectChannelFirst: 'Select a channel first',
  taskPlaceholder: 'Describe the task, constraints, and expected result…',
  runningAgentsHint: 'Running agents will receive this task.',
  noRunningAgentHint: 'Add or start an agent before sending.',
  running: 'Running…',
  runTask: 'Run task',
  channelAgents: 'Channel agents',
  agents: 'Agents',
  runtime: 'runtime',
  model: 'model',
  state: 'state',
  defaultModel: 'default',
  stopDelivery: 'Stop delivery',
  startAgent: 'Start agent',
  noAgents: 'No agent is attached to this channel yet.',
  addAgent: 'Add agent',
  name: 'Name',
  agentPlaceholder: 'builder',
  runtimeLabel: 'Runtime',
  noRuntimeInstalled: 'No runtime installed',
  modelLabel: 'Model',
  runtimeDefault: 'runtime default',
  adding: 'Adding…',
  addRunningAgent: 'Add running agent',
  language: 'Language',
  appearance: 'Appearance',
  lightMode: 'Light',
  darkMode: 'Dark',
  selectOption: 'Select an option',
}

const chinese: Record<keyof typeof english, string> = {
  loading: '正在启动本地工作区…',
  brand: 'cocli 本地',
  serviceOnline: 'SQLite + HTTP 已连接',
  dismiss: '关闭',
  channelsAndRuntimes: '频道与运行时',
  channels: '频道',
  channelsNav: '频道',
  noChannels: '创建一个频道，开启首个本地工作区。',
  newChannel: '新建频道',
  channelPlaceholder: 'product-loop',
  add: '添加',
  runtimes: '本地运行时',
  installed: '已安装',
  unavailable: '不可用',
  noRuntimes: '尚未配置运行时目录。',
  conversation: '频道对话',
  localChannel: '本地频道',
  chooseChannel: '选择一个频道',
  agentCount: '{{count}} 个 Agent',
  agentCountPlural: '{{count}} 个 Agent',
  createChannel: '创建频道',
  createChannelDescription: '频道、Agent 和消息记录会保存在本机 SQLite 数据库中。',
  loadingMessages: '正在加载消息记录…',
  startTask: '开始真实任务',
  startTaskDescription: '添加一个已安装的运行时 Agent，然后在下方输入明确任务。',
  you: '你',
  agent: 'Agent',
  taskFor: '#{{channel}} 的任务',
  selectChannelFirst: '请先选择频道',
  taskPlaceholder: '描述任务、约束条件和预期结果…',
  runningAgentsHint: '运行中的 Agent 将收到此任务。',
  noRunningAgentHint: '请先添加或启动一个 Agent。',
  running: '执行中…',
  runTask: '运行任务',
  channelAgents: '频道 Agent',
  agents: 'Agent',
  runtime: '运行时',
  model: '模型',
  state: '状态',
  defaultModel: '默认',
  stopDelivery: '停止接收',
  startAgent: '启动 Agent',
  noAgents: '此频道还没有关联 Agent。',
  addAgent: '添加 Agent',
  name: '名称',
  agentPlaceholder: 'builder',
  runtimeLabel: '运行时',
  noRuntimeInstalled: '没有已安装的运行时',
  modelLabel: '模型',
  runtimeDefault: '运行时默认值',
  adding: '添加中…',
  addRunningAgent: '添加并启动 Agent',
  language: '语言',
  appearance: '外观',
  lightMode: '亮色',
  darkMode: '暗色',
  selectOption: '选择一个选项',
}

const dictionaries = {
  en: english,
  'zh-CN': chinese,
}

export type LocalCopyKey = keyof typeof english

export function translate(
  language: LocalLanguage,
  key: LocalCopyKey,
  values: Record<string, string | number> = {},
): string {
  let copy: string = dictionaries[language][key]
  for (const [name, value] of Object.entries(values)) {
    copy = copy.replaceAll(`{{${name}}}`, String(value))
  }
  return copy
}

export function resolveInitialLanguage(): LocalLanguage {
  try {
    const stored = localStorage.getItem('cocli-local-language')
    if (stored === 'en' || stored === 'zh-CN') return stored
  } catch {
    // Storage can be unavailable in privacy-restricted environments.
  }
  return navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'
}
