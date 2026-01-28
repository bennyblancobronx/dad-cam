// Dad Cam - Phase 3 Video Player Component
import { useRef, useState, useEffect, useCallback } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import type { ClipView } from '../types/clips';
import { toAssetUrl } from '../utils/paths';
import { exportStill } from '../api/stills';

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
  const [isExportingStill, setIsExportingStill] = useState(false);
  const [stillExportStatus, setStillExportStatus] = useState<string | null>(null);

  const proxyUrl = toAssetUrl(clip.proxyPath);

  // Export still frame at current timestamp
  const handleExportStill = useCallback(async () => {
    if (!videoRef.current || isExportingStill) return;

    const timestampMs = Math.floor(videoRef.current.currentTime * 1000);

    // Pause video for frame selection
    videoRef.current.pause();

    try {
      // Open save dialog
      const defaultName = `${clip.title.replace(/[/\\:*?"<>|]/g, '_')}_frame`;
      const savePath = await save({
        title: 'Save Still Frame',
        defaultPath: `${defaultName}.jpg`,
        filters: [
          { name: 'JPEG Image', extensions: ['jpg', 'jpeg'] },
          { name: 'PNG Image', extensions: ['png'] },
        ],
      });

      if (!savePath) {
        // User cancelled
        return;
      }

      // Determine format from extension
      const format = savePath.toLowerCase().endsWith('.png') ? 'png' : 'jpg';

      setIsExportingStill(true);
      setStillExportStatus('Exporting...');

      const result = await exportStill({
        clipId: clip.id,
        timestampMs,
        outputPath: savePath,
        format,
      });

      setStillExportStatus(`Saved: ${result.outputPath.split('/').pop()}`);
      setTimeout(() => setStillExportStatus(null), 3000);
    } catch (err) {
      const message = typeof err === 'string' ? err : err instanceof Error ? err.message : 'Export failed';
      setStillExportStatus(`Error: ${message}`);
      setTimeout(() => setStillExportStatus(null), 5000);
    } finally {
      setIsExportingStill(false);
    }
  }, [clip.id, clip.title, isExportingStill]);

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
        case 's':
          e.preventDefault();
          handleExportStill();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, onNext, onPrevious, handleExportStill]);

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
              title={clip.isFavorite ? 'Remove from favorites' : 'Add to favorites'}
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
              title={clip.isBad ? 'Unmark as bad clip' : 'Mark as bad clip'}
            >
              {clip.isBad ? '\u2718 Marked Bad' : '\u2717 Mark Bad'}
            </button>
            <button
              onClick={handleExportStill}
              disabled={isExportingStill}
              style={{
                background: 'none',
                border: '1px solid #444',
                borderRadius: '4px',
                padding: '4px 8px',
                cursor: isExportingStill ? 'not-allowed' : 'pointer',
                color: isExportingStill ? '#666' : '#88ccff',
              }}
              title="Export Still Frame (S)"
            >
              {isExportingStill ? 'Exporting...' : 'Still (S)'}
            </button>
          </div>
          {/* Still export status */}
          {stillExportStatus && (
            <span style={{ color: stillExportStatus.startsWith('Error') ? '#ff8888' : '#88ff88', fontSize: '14px', marginLeft: '16px' }}>
              {stillExportStatus}
            </span>
          )}
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
          title="Close player (Escape)"
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
            title={isPlaying ? 'Pause (Space or K)' : 'Play (Space or K)'}
          >
            {isPlaying ? 'Pause' : 'Play'}
          </button>

          <span style={{ color: '#888' }}>
            {formatTime(currentTime)} / {formatTime(duration)}
          </span>

          <div style={{ flex: 1 }} />

          <span style={{ color: '#666', fontSize: '12px' }}>
            <span className="kbd">Space</span>/<span className="kbd">K</span> Play
            {' | '}
            <span className="kbd">J</span>/<span className="kbd">L</span> Seek
            {' | '}
            <span className="kbd">M</span> Mute
            {' | '}
            <span className="kbd">F</span> Fullscreen
            {' | '}
            <span className="kbd">S</span> Still
            {' | '}
            <span className="kbd">Esc</span> Close
          </span>
        </div>
      </div>
    </div>
  );
}
