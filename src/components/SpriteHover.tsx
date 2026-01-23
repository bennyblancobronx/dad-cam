// Dad Cam - Phase 3 Sprite Hover Scrubbing Component
import { useState, useEffect, useCallback, useRef } from 'react';
import type { SpriteMetadata } from '../types/clips';

interface SpriteHoverProps {
  spriteUrl: string | null;
  spriteMetaUrl: string | null;
  width: number;
  height: number;
}

export function SpriteHover({
  spriteUrl,
  spriteMetaUrl,
  width,
  height,
}: SpriteHoverProps) {
  const [metadata, setMetadata] = useState<SpriteMetadata | null>(null);
  const [currentFrame, setCurrentFrame] = useState(0);
  const [spriteLoaded, setSpriteLoaded] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Load sprite metadata
  useEffect(() => {
    if (!spriteMetaUrl) return;

    fetch(spriteMetaUrl)
      .then(res => res.json())
      .then((data: SpriteMetadata) => {
        setMetadata(data);
      })
      .catch(err => {
        console.error('Failed to load sprite metadata:', err);
      });
  }, [spriteMetaUrl]);

  // Preload sprite image
  useEffect(() => {
    if (!spriteUrl) return;

    const img = new Image();
    img.onload = () => setSpriteLoaded(true);
    img.src = spriteUrl;
  }, [spriteUrl]);

  // Handle mouse movement
  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!metadata || !containerRef.current) return;

    const rect = containerRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const percentage = Math.max(0, Math.min(1, x / rect.width));

    // Calculate frame index
    const frameIndex = Math.floor(percentage * metadata.frame_count);
    setCurrentFrame(Math.min(frameIndex, metadata.frame_count - 1));
  }, [metadata]);

  if (!spriteUrl || !spriteLoaded || !metadata) {
    return (
      <div
        style={{
          width: '100%',
          height: `${height}px`,
          backgroundColor: '#2a2a2a',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <span style={{ color: '#666', fontSize: '12px' }}>Loading...</span>
      </div>
    );
  }

  // Calculate which tile to show based on current frame
  const col = currentFrame % metadata.columns;
  const row = Math.floor(currentFrame / metadata.columns);
  const xOffset = col * metadata.tile_width;
  const yOffset = row * metadata.tile_height;

  // Calculate scale to fit container
  const scale = Math.min(
    width / metadata.tile_width,
    height / metadata.tile_height
  );

  return (
    <div
      ref={containerRef}
      onMouseMove={handleMouseMove}
      style={{
        width: '100%',
        height: `${height}px`,
        overflow: 'hidden',
        position: 'relative',
        backgroundColor: '#1a1a1a',
      }}
    >
      <div
        style={{
          width: `${metadata.tile_width}px`,
          height: `${metadata.tile_height}px`,
          backgroundImage: `url(${spriteUrl})`,
          backgroundPosition: `-${xOffset}px -${yOffset}px`,
          backgroundRepeat: 'no-repeat',
          transform: `scale(${scale})`,
          transformOrigin: 'top left',
        }}
      />

      {/* Frame indicator */}
      <div
        style={{
          position: 'absolute',
          bottom: '4px',
          left: '4px',
          backgroundColor: 'rgba(0, 0, 0, 0.7)',
          color: 'white',
          padding: '2px 6px',
          borderRadius: '4px',
          fontSize: '11px',
        }}
      >
        {currentFrame + 1}/{metadata.frame_count}
      </div>

      {/* Progress bar */}
      <div
        style={{
          position: 'absolute',
          bottom: 0,
          left: 0,
          right: 0,
          height: '3px',
          backgroundColor: 'rgba(255, 255, 255, 0.2)',
        }}
      >
        <div
          style={{
            width: `${((currentFrame + 1) / metadata.frame_count) * 100}%`,
            height: '100%',
            backgroundColor: '#4a9eff',
            transition: 'width 50ms ease-out',
          }}
        />
      </div>
    </div>
  );
}
