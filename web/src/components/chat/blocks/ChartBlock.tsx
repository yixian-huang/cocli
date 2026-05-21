import type { BlockProps } from './types'
import type { ChartBlockData } from './types'
import {
  BarChart, Bar, LineChart, Line, PieChart, Pie, Cell,
  XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer,
} from 'recharts'

const DEFAULT_COLORS = ['#6366f1', '#22c55e', '#f59e0b', '#ef4444', '#ec4899', '#14b8a6', '#8b5cf6', '#0ea5e9']

export function ChartBlock({ data }: BlockProps) {
  const d = data as unknown as ChartBlockData

  const chartData = d.labels.map((label, i) => {
    const point: Record<string, string | number> = { name: label }
    d.datasets.forEach((ds) => {
      point[ds.label] = ds.values[i] ?? 0
    })
    return point
  })

  if (d.chart_type === 'pie' || d.chart_type === 'doughnut') {
    const pieData = d.labels.map((label, i) => ({
      name: label,
      value: d.datasets[0]?.values[i] ?? 0,
    }))
    const innerRadius = d.chart_type === 'doughnut' ? 40 : 0

    return (
      <div className="rounded-lg bg-card p-4">
        {d.title && <div className="text-sm font-semibold mb-3">{d.title}</div>}
        <ResponsiveContainer width="100%" height={200}>
          <PieChart>
            <Pie data={pieData} dataKey="value" nameKey="name" cx="50%" cy="50%" innerRadius={innerRadius} outerRadius={80} label>
              {pieData.map((_, i) => (
                <Cell key={i} fill={d.datasets[0]?.color || DEFAULT_COLORS[i % DEFAULT_COLORS.length]} />
              ))}
            </Pie>
            <Tooltip />
            <Legend />
          </PieChart>
        </ResponsiveContainer>
      </div>
    )
  }

  const ChartComponent = d.chart_type === 'line' ? LineChart : BarChart

  return (
    <div className="rounded-lg bg-card p-4">
      {d.title && <div className="text-sm font-semibold mb-3">{d.title}</div>}
      <ResponsiveContainer width="100%" height={200}>
        <ChartComponent data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#ffffff10" />
          <XAxis dataKey="name" tick={{ fontSize: 11 }} stroke="#666" label={d.x_label ? { value: d.x_label, position: 'insideBottom', offset: -5, fontSize: 11 } : undefined} />
          <YAxis tick={{ fontSize: 11 }} stroke="#666" label={d.y_label ? { value: d.y_label, angle: -90, position: 'insideLeft', fontSize: 11 } : undefined} />
          <Tooltip />
          <Legend />
          {d.datasets.map((ds, i) =>
            d.chart_type === 'line' ? (
              <Line key={i} type="monotone" dataKey={ds.label} stroke={ds.color || DEFAULT_COLORS[i % DEFAULT_COLORS.length]} />
            ) : (
              <Bar key={i} dataKey={ds.label} fill={ds.color || DEFAULT_COLORS[i % DEFAULT_COLORS.length]} />
            )
          )}
        </ChartComponent>
      </ResponsiveContainer>
    </div>
  )
}
