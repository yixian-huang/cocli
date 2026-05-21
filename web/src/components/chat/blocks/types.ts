// web/src/components/chat/blocks/types.ts

export interface Block {
  type: string
  data: Record<string, unknown>
}

export interface BlockProps {
  data: Record<string, unknown>
  messageId: string
}

// Typed data shapes for each block type
export interface MarkdownBlockData {
  content: string
  language_hint?: string
}

export interface ImageGalleryBlockData {
  images: { url: string; alt?: string; caption?: string }[]
  layout?: 'grid' | 'carousel'
}

export interface ChartBlockData {
  chart_type: 'bar' | 'line' | 'pie' | 'doughnut'
  title?: string
  labels: string[]
  datasets: { label: string; values: number[]; color?: string }[]
  x_label?: string
  y_label?: string
}

export interface InfoCardBlockData {
  title: string
  icon?: string
  status?: 'success' | 'error' | 'warning' | 'info'
  fields?: { label: string; value: string }[]
  description?: string
}

export interface CollapsibleBlockData {
  title: string
  content: string
  content_type?: 'markdown' | 'text'
  default_open?: boolean
}

export interface TabsBlockData {
  tabs: { label: string; content: string; content_type?: 'markdown' | 'text' }[]
  default_tab?: number
}

export interface ButtonsBlockData {
  prompt?: string
  actions: { id: string; label: string; style?: 'primary' | 'success' | 'danger' | 'secondary'; value?: string }[]
  acted_by?: string
  acted_by_name?: string
  acted_at?: string
  acted_value?: string
}

export interface FormBlockData {
  title?: string
  submit_label?: string
  fields: {
    id: string
    type: 'text' | 'textarea' | 'select' | 'checkbox'
    label: string
    required?: boolean
    placeholder?: string
    options?: string[]
    default?: string | boolean
  }[]
  acted_by?: string
  acted_by_name?: string
  acted_at?: string
}

export interface BlockActionBlockData {
  source_message_id: string
  source_block_type: 'buttons' | 'form'
  action_id: string
  value?: string
  form_data?: Record<string, unknown>
}
