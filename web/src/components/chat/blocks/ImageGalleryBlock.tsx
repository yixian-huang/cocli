import { useState } from 'react'
import type { BlockProps } from './types'
import type { ImageGalleryBlockData } from './types'
import { ImageLightbox } from '../ImageLightbox'

export function ImageGalleryBlock({ data }: BlockProps) {
  const d = data as unknown as ImageGalleryBlockData
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null)
  const maxShow = 4
  const showImages = d.images.slice(0, maxShow)
  const remaining = d.images.length - maxShow

  return (
    <>
      <div className={`grid gap-1 rounded-lg overflow-hidden ${showImages.length === 1 ? 'grid-cols-1' : 'grid-cols-2'}`}>
        {showImages.map((img, i) => (
          <div
            key={i}
            className="relative aspect-video bg-card cursor-pointer hover:opacity-90 transition-opacity overflow-hidden"
            onClick={() => setLightboxSrc(img.url)}
          >
            <img src={img.url} alt={img.alt || ''} className="w-full h-full object-cover" />
            {img.caption && (
              <div className="absolute bottom-0 left-0 right-0 px-2 py-1 bg-gradient-to-t from-black/70 text-xs text-white">
                {img.caption}
              </div>
            )}
            {i === maxShow - 1 && remaining > 0 && (
              <div className="absolute inset-0 bg-black/60 flex items-center justify-center text-white text-lg font-bold">
                +{remaining} more
              </div>
            )}
          </div>
        ))}
      </div>
      {lightboxSrc && <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />}
    </>
  )
}
