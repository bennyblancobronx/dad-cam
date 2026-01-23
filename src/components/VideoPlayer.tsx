// Dad Cam - Phase 3 Video Player Component
import { useRef, useState, useEffect, useCallback } from 'react';
import type { ClipView } from '../types/clips';
import { toAssetUrl } from '../utils/paths';

interface VideoPlayerProps {
  clip: ClipView;
  onClose: () => void;
  onPrevious?: () => void;
  onNext?: () => void;
  onFavoriteToggle: () => void;
  onBadToggle: () => void;
}

export function VideoPlayer({
  clip,
  onClose,
  onPrevious,
  onNext,
  onFavoriteToggle,
  onBadToggle,
}: VideoPlayerProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);

  const proxyUrl = toAssetUrl(clip.proxyPath);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }

      switch (e.key) {
        case ' ':
        case 'k':
          e.preventDefault();
          togglePlayPause();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          if (e.shiftKey) {
            seek(-10);
          } else {
            seek(-5);
          }
          break;
        case 'ArrowRight':
          e.preventDefault();
          if (e.shiftKey) {
            seek(10);
          } else {
            seek(5);
          }
          break;
        case 'ArrowUp':
          e.preventDefault();
          adjustVolume(0.1);
          break;
        case 'ArrowDown':
          e.preventDefault();
          adjustVolume(-0.1);
          break;
        case 'j':
          e.preventDefault();
          seek(-10);
          break;
        case 'l':
          e.preventDefault();
          seek(10);
          break;
        case 'm':
          e.preventDefault();
          toggleMute();
          break;
        case 'f':
          e.preventDefault();
          toggleFullscreen();
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
        case 'n':
          e.preventDefault();
          if (onNext) onNext();
          break;
        case 'p':
          e.preventDefault();
          if (onPrevious) onPrevious();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, onNext, onPrevious]);

  const togglePlayPause = useCallback(() => {
    if (videoRef.current) {
      if (isPlaying) {
        videoRef.current.pause();
      } else {
        videoRef.current.play();
      }
    }
  }, [isPlaying]);

  const seek = useCallback((seconds: number) => {
    if (videoRef.current) {
      videoRef.current.currentTime = Math.max(0, Math.min(duration, videoRef.current.currentTime + seconds));
    }
  }, [duration]);

  const adjustVolume = useCallback((delta: number) => {
    if (videoRef.current) {
      const newVolume = Math.max(0, Math.min(1, volume + delta));
      videoRef.current.volume = newVolume;
      setVolume(newVolume);
    }
  }, [volume]);

  const toggleMute = useCallback(() => {
    if (videoRef.current) {
      videoRef.current.muted = !videoRef.current.muted;
    }
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (videoRef.current) {
      if (document.fullscreenElement) {
        document.exitFullscreen();
      } else {
        videoRef.current.requestFullscreen();
      }
    }
  }, []);

  const handleTimeUpdate = useCallback(() => {
    if (videoRef.current) {
      setCurrentTime(videoRef.current.currentTime);
    }
  }, []);

  const handleLoadedMetadata = useCallback(() => {
    if (videoRef.current) {
      setDuration(videoRef.current.duration);
    }
  }, []);

  const handleSeek = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    if (videoRef.current) {
      videoRef.current.currentTime = parseFloat(e.target.value);
    }
  }, []);

  const formatTime = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <div
      className="video-player-overlay"
      style={{
        position: 'fixed',
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.95)',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 1000,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          padding: '16px 24px',
          borderBottom: '1px solid #333',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
          <h2 style={{ color: 'white', margin: 0, fontSize: '18px' }}>{clip.title}</h2>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              onClick={onFavoriteToggle}
              style={{
                background: 'none',
                border: '1px solid #444',
                borderRadius: '4px',
                padding: '4px 8px',
                cursor: 'pointer',
                color: clip.isFavorite ? '#ff4444' : '#888',
              }}
            >
              {clip.isFavorite ? '\u2665 Favorited' : '\u2661 Favorite'}
            </button>
            <button
              onClick={onBadToggle}
              style={{
                background: 'none',
                border: '1px solid #444',
                borderRadius: '4px',
                padding: '4px 8px',
                cursor: 'pointer',
                color: clip.isBad ? '#ffaa00' : '#888',
              }}
            >
              {clip.isBad ? '\u2718 Marked Bad' : '\u2717 Mark Bad'}
            </button>
          </div>
        </div>
        <button
          onClick={onClose}
          style={{
            background: 'none',
            border: 'none',
            color: 'white',
            fontSize: '24px',
            cursor: 'pointer',
            padding: '4px 8px',
          }}
        >
          ×
        </button>
      </div>

      {/* Video Container */}
      <div
        style={{
          flex: 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          position: 'relative',
        }}
      >
        {/* Previous button */}
        {onPrevious && (
          <button
            onClick={onPrevious}
            style={{
              position: 'absolute',
              left: '16px',
              background: 'rgba(0, 0, 0, 0.5)',
              border: 'none',
              borderRadius: '50%',
              width: '48px',
              height: '48px',
              color: 'white',
              fontSize: '24px',
              cursor: 'pointer',
            }}
            title="Previous (P)"
          >
            {'<'}
          </button>
        )}

        {proxyUrl ? (
          <video
            ref={videoRef}
            src={proxyUrl}
            style={{
              maxWidth: '90%',
              maxHeight: '80vh',
            }}
            onClick={togglePlayPause}
            onPlay={() => setIsPlaying(true)}
            onPause={() => setIsPlaying(false)}
            onTimeUpdate={handleTimeUpdate}
            onLoadedMetadata={handleLoadedMetadata}
            autoPlay
          />
        ) : (
          <div style={{ color: '#888' }}>No proxy available for playback</div>
        )}

        {/* Next button */}
        {onNext && (
          <button
            onClick={onNext}
            style={{
              position: 'absolute',
              right: '16px',
              background: 'rgba(0, 0, 0, 0.5)',
              border: 'none',
              borderRadius: '50%',
              width: '48px',
              height: '48px',
              color: 'white',
              fontSize: '24px',
              cursor: 'pointer',
            }}
            title="Next (N)"
          >
            {'>'}
          </button>
        )}
      </div>

      {/* Controls */}
      <div
        style={{
          padding: '16px 24px',
          borderTop: '1px solid #333',
        }}
      >
        {/* Progress bar */}
        <div style={{ marginBottom: '12px' }}>
          <input
            type="range"
            min={0}
            max={duration || 100}
            value={currentTime}
            onChange={handleSeek}
            style={{
              width: '100%',
              cursor: 'pointer',
            }}
          />
        </div>

        {/* Control buttons */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
          <button
            onClick={togglePlayPause}
            style={{
              background: 'none',
              border: '1px solid #444',
              borderRadius: '4px',
              padding: '8px 16px',
              cursor: 'pointer',
              color: 'white',
            }}
          >
            {isPlaying ? 'Pause' : 'Play'}
          </button>

          <span style={{ color: '#888' }}>
            {formatTime(currentTime)} / {formatTime(duration)}
          </span>

          <div style={{ flex: 1 }} />

          <span style={{ color: '#666', fontSize: '12px' }}>
            Space/K: Play | J/L: Seek | M: Mute | F: Fullscreen | ESC: Close
          </span>
        </div>
      </div>
    </div>
  );
}
