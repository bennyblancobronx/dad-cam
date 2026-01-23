// Dad Cam - Phase 3 Clip Thumbnail Component
import { useState, useCallback } from 'react';
import type { ClipView } from '../types/clips';
import { toAssetUrl, getSpriteMetaPath } from '../utils/paths';
import { SpriteHover } from './SpriteHover';

interface ClipThumbnailProps {
  clip: ClipView;
  width: number;
  height: number;
  onClick: () => void;
  onFavoriteToggle: () => void;
  onBadToggle: () => void;
}

export function ClipThumbnail({
  clip,
  width,
  height,
  onClick,
  onFavoriteToggle,
  onBadToggle,
}: ClipThumbnailProps) {
  const [isHovering, setIsHovering] = useState(false);
  const [imageError, setImageError] = useState(false);

  const thumbUrl = toAssetUrl(clip.thumbPath);
  const spriteUrl = toAssetUrl(clip.spritePath);
  const spriteMetaUrl = toAssetUrl(getSpriteMetaPath(clip.spritePath));

  const handleMouseEnter = useCallback(() => {
    setIsHovering(true);
  }, []);

  const handleMouseLeave = useCallback(() => {
    setIsHovering(false);
  }, []);

  const formatDuration = (ms: number | null): string => {
    if (!ms) return '';
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
  };

  const infoBarHeight = 40;
  const imageHeight = height - infoBarHeight;

  return (
    <div
      className="clip-thumbnail"
      style={{
        width: `${width}px`,
        height: `${height}px`,
        position: 'relative',
        cursor: 'pointer',
        borderRadius: '8px',
        overflow: 'hidden',
        backgroundColor: '#1a1a1a',
        flexShrink: 0,
      }}
      onClick={onClick}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      {/* Thumbnail or Sprite Hover */}
      {isHovering && spriteUrl && clip.mediaType === 'video' ? (
        <SpriteHover
          spriteUrl={spriteUrl}
          spriteMetaUrl={spriteMetaUrl}
          width={width}
          height={imageHeight}
        />
      ) : (
        <div
          className="thumbnail-image"
          style={{
            width: '100%',
            height: `${imageHeight}px`,
            backgroundImage: thumbUrl && !imageError ? `url(${thumbUrl})` : 'none',
            backgroundSize: 'cover',
            backgroundPosition: 'center',
            backgroundColor: '#2a2a2a',
          }}
        >
          {/* Hidden img for error detection */}
          {thumbUrl && (
            <img
              src={thumbUrl}
              alt=""
              style={{ display: 'none' }}
              onError={() => setImageError(true)}
            />
          )}
          {(!thumbUrl || imageError) && (
            <div style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              height: '100%',
              color: '#666',
              fontSize: '12px',
            }}>
              No Preview
            </div>
          )}
        </div>
      )}

      {/* Duration badge */}
      {clip.durationMs && (
        <div
          className="duration-badge"
          style={{
            position: 'absolute',
            bottom: `${infoBarHeight + 8}px`,
            right: '8px',
            backgroundColor: 'rgba(0, 0, 0, 0.7)',
            color: 'white',
            padding: '2px 6px',
            borderRadius: '4px',
            fontSize: '12px',
          }}
        >
          {formatDuration(clip.durationMs)}
        </div>
      )}

      {/* Info bar */}
      <div
        className="info-bar"
        style={{
          position: 'absolute',
          bottom: 0,
          left: 0,
          right: 0,
          height: `${infoBarHeight}px`,
          backgroundColor: '#1a1a1a',
          padding: '4px 8px',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <span
          className="title"
          style={{
            color: 'white',
            fontSize: '13px',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            flex: 1,
            marginRight: '8px',
          }}
        >
          {clip.title}
        </span>

        {/* Tag buttons */}
        <div className="tag-buttons" style={{ display: 'flex', gap: '4px' }}>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onFavoriteToggle();
            }}
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              padding: '4px',
              color: clip.isFavorite ? '#ff4444' : '#666',
              fontSize: '16px',
            }}
            title={clip.isFavorite ? 'Remove from favorites' : 'Add to favorites'}
          >
            {clip.isFavorite ? '\u2665' : '\u2661'}
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onBadToggle();
            }}
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              padding: '4px',
              color: clip.isBad ? '#ffaa00' : '#666',
              fontSize: '16px',
            }}
            title={clip.isBad ? 'Unmark as bad' : 'Mark as bad'}
          >
            {clip.isBad ? '\u2718' : '\u2717'}
          </button>
        </div>
      </div>

      {/* Hover indicator */}
      {isHovering && (
        <div
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            border: '2px solid #4a9eff',
            borderRadius: '8px',
            pointerEvents: 'none',
          }}
        />
      )}
    </div>
  );
}
