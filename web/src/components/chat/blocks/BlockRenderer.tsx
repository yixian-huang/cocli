import { lazy, Suspense } from 'react'
import type { Block, BlockProps } from './types'
import { FallbackBlock } from './FallbackBlock'

const MarkdownBlock = lazy(() => import('./MarkdownBlock').then(m => ({ default: m.MarkdownBlock })))
const ImageGalleryBlock = lazy(() => import('./ImageGalleryBlock').then(m => ({ default: m.ImageGalleryBlock })))
const ChartBlock = lazy(() => import('./ChartBlock').then(m => ({ default: m.ChartBlock })))
const InfoCardBlock = lazy(() => import('./InfoCardBlock').then(m => ({ default: m.InfoCardBlock })))
const CollapsibleBlock = lazy(() => import('./CollapsibleBlock').then(m => ({ default: m.CollapsibleBlock })))
const TabsBlock = lazy(() => import('./TabsBlock').then(m => ({ default: m.TabsBlock })))
const ButtonsBlock = lazy(() => import('./ButtonsBlock').then(m => ({ default: m.ButtonsBlock })))
const FormBlock = lazy(() => import('./FormBlock').then(m => ({ default: m.FormBlock })))
const BlockActionBlock = lazy(() => import('./BlockActionBlock').then(m => ({ default: m.BlockActionBlock })))

const BLOCK_COMPONENTS: Record<string, React.LazyExoticComponent<React.FC<BlockProps>>> = {
  markdown: MarkdownBlock,
  image_gallery: ImageGalleryBlock,
  chart: ChartBlock,
  info_card: InfoCardBlock,
  collapsible: CollapsibleBlock,
  tabs: TabsBlock,
  buttons: ButtonsBlock,
  form: FormBlock,
  block_action: BlockActionBlock,
}

interface Props {
  blocks: Block[]
  messageId: string
}

export function BlockRenderer({ blocks, messageId }: Props) {
  return (
    <div className="flex flex-col gap-3">
      {blocks.map((block, i) => {
        const LazyComponent = BLOCK_COMPONENTS[block.type]
        if (!LazyComponent) {
          return <FallbackBlock key={i} />
        }
        return (
          <Suspense key={i} fallback={<div className="h-8 animate-pulse rounded bg-accent/30" />}>
            <LazyComponent data={block.data} messageId={messageId} />
          </Suspense>
        )
      })}
    </div>
  )
}
