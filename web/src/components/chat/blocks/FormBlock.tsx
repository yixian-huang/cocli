import { useState } from 'react'
import type { BlockProps } from './types'
import type { FormBlockData } from './types'
import { messages } from '@/api/client'

export function FormBlock({ data, messageId }: BlockProps) {
  const d = data as unknown as FormBlockData
  const acted = !!d.acted_by
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [values, setValues] = useState<Record<string, string | boolean>>(() => {
    const init: Record<string, string | boolean> = {}
    for (const f of d.fields) {
      if (f.type === 'checkbox') {
        init[f.id] = (f.default as boolean) ?? false
      } else {
        init[f.id] = (f.default as string) ?? ''
      }
    }
    return init
  })

  const handleSubmit = async () => {
    setLoading(true)
    setError(null)
    try {
      await messages.blockAction(messageId, {
        action_id: 'form_submit',
        form_data: values as Record<string, unknown>,
      })
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Submit failed')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="rounded-lg bg-card p-4">
      {d.title && <div className="text-sm font-semibold mb-3">{d.title}</div>}
      <div className="flex flex-col gap-3">
        {d.fields.map((field) => (
          <div key={field.id}>
            <label className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              {field.label}
              {field.required && <span className="text-red-500">*</span>}
            </label>
            {field.type === 'text' && (
              <input
                type="text"
                disabled={acted}
                placeholder={field.placeholder}
                value={values[field.id] as string}
                onChange={(e) => setValues({ ...values, [field.id]: e.target.value })}
                className="w-full px-3 py-1.5 rounded-md bg-background border border-border text-sm focus:border-primary outline-none disabled:opacity-50"
              />
            )}
            {field.type === 'textarea' && (
              <textarea
                disabled={acted}
                placeholder={field.placeholder}
                value={values[field.id] as string}
                onChange={(e) => setValues({ ...values, [field.id]: e.target.value })}
                rows={3}
                className="w-full px-3 py-1.5 rounded-md bg-background border border-border text-sm focus:border-primary outline-none disabled:opacity-50 resize-y"
              />
            )}
            {field.type === 'select' && (
              <select
                disabled={acted}
                value={values[field.id] as string}
                onChange={(e) => setValues({ ...values, [field.id]: e.target.value })}
                className="w-full px-3 py-1.5 rounded-md bg-background border border-border text-sm focus:border-primary outline-none disabled:opacity-50"
              >
                <option value="">Select...</option>
                {field.options?.map((opt) => (
                  <option key={opt} value={opt}>{opt}</option>
                ))}
              </select>
            )}
            {field.type === 'checkbox' && (
              <label className="flex items-center gap-2 text-sm cursor-pointer">
                <input
                  type="checkbox"
                  disabled={acted}
                  checked={values[field.id] as boolean}
                  onChange={(e) => setValues({ ...values, [field.id]: e.target.checked })}
                  className="accent-primary"
                />
                {field.label}
              </label>
            )}
          </div>
        ))}
      </div>
      <div className="mt-3">
        <button
          onClick={handleSubmit}
          disabled={acted || loading}
          className="px-4 py-1.5 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {d.submit_label || 'Submit'}
        </button>
      </div>
      {acted && (
        <div className="mt-2 text-xs text-green-500">
          &#10003; Submitted by @{d.acted_by_name} &middot; {d.acted_at ? new Date(d.acted_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : ''}
        </div>
      )}
      {error && <div className="mt-2 text-xs text-red-500">{error}</div>}
    </div>
  )
}
