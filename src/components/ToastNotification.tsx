// Dad Cam - Toast Notification Component
// Shows auto-dismissing notifications for background job failures.

import { useState, useEffect, useCallback, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';

interface Toast {
  id: number;
  message: string;
  timestamp: number;
}

interface JobProgressPayload {
  jobId: string;
  phase: string;
  message: string;
  isError: boolean;
  errorMessage: string | null;
}

const MAX_TOASTS = 3;
const AUTO_DISMISS_MS = 8000;

export function ToastNotification() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(0);

  const addToast = useCallback((message: string) => {
    const id = nextId.current++;
    setToasts((prev) => {
      const updated = [...prev, { id, message, timestamp: Date.now() }];
      // Keep only the most recent MAX_TOASTS
      return updated.length > MAX_TOASTS ? updated.slice(-MAX_TOASTS) : updated;
    });

    // Auto-dismiss
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, AUTO_DISMISS_MS);
  }, []);

  const dismissToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  // Listen for job-progress events with isError
  useEffect(() => {
    let cleanup: (() => void) | undefined;

    listen<JobProgressPayload>('job-progress', (event) => {
      const p = event.payload;
      if (p.isError) {
        const msg = p.errorMessage || p.message || `Job ${p.jobId} failed`;
        addToast(msg);
      }
    }).then((unlisten) => {
      cleanup = unlisten;
    });

    return () => { cleanup?.(); };
  }, [addToast]);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container">
      {toasts.map((toast) => (
        <div key={toast.id} className="toast toast-error">
          <span className="toast-message">{toast.message}</span>
          <button
            className="toast-dismiss"
            onClick={() => dismissToast(toast.id)}
            aria-label="Dismiss"
          >
            &times;
          </button>
        </div>
      ))}
    </div>
  );
}
