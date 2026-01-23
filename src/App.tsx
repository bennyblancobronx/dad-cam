// Dad Cam - Phase 3 Main App
import { useState, useCallback } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import type { LibraryInfo } from './types/clips';
import { openLibrary, closeLibrary, createLibrary } from './api/clips';
import { clearThumbnailCache } from './utils/thumbnailCache';
import { LibraryView } from './components/LibraryView';
import './App.css';

function App() {
  const [library, setLibrary] = useState<LibraryInfo | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [libraryPath, setLibraryPath] = useState('');
  const [newLibraryName, setNewLibraryName] = useState('');
  const [showCreateForm, setShowCreateForm] = useState(false);

  // Open existing library via native folder picker
  const handleOpenLibrary = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      // Open native folder picker dialog
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Dad Cam Library',
      });

      if (!selected) {
        // User cancelled
        setIsLoading(false);
        return;
      }

      const lib = await openLibrary(selected as string);
      clearThumbnailCache();
      setLibrary(lib);
      setLibraryPath(selected as string);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to open library');
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Browse for folder (used by create form)
  const handleBrowseFolder = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder for New Library',
      });
      if (selected) {
        setLibraryPath(selected as string);
      }
    } catch (err) {
      console.error('Failed to open folder picker:', err);
    }
  }, []);

  // Create new library
  const handleCreateLibrary = useCallback(async () => {
    if (!libraryPath.trim() || !newLibraryName.trim()) {
      setError('Please select a folder and enter a library name');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const lib = await createLibrary(libraryPath.trim(), newLibraryName.trim());
      clearThumbnailCache();
      setLibrary(lib);
      setShowCreateForm(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create library');
    } finally {
      setIsLoading(false);
    }
  }, [libraryPath, newLibraryName]);

  // Close library
  const handleCloseLibrary = useCallback(async () => {
    try {
      await closeLibrary();
      clearThumbnailCache();
      setLibrary(null);
      setLibraryPath('');
      setError(null);
    } catch (err) {
      console.error('Failed to close library:', err);
    }
  }, []);

  // If library is open, show library view
  if (library) {
    return <LibraryView library={library} onClose={handleCloseLibrary} />;
  }

  // Otherwise show welcome/open screen
  return (
    <div className="app-welcome">
      <div className="welcome-container">
        <h1 className="welcome-title">Dad Cam</h1>
        <p className="welcome-subtitle">Video library for dad cam footage</p>

        {error && (
          <div className="error-message">
            {error}
          </div>
        )}

        {!showCreateForm ? (
          <div className="open-form">
            <div className="button-group">
              <button
                className="primary-button"
                onClick={handleOpenLibrary}
                disabled={isLoading}
              >
                {isLoading ? 'Opening...' : 'Open Library'}
              </button>
              <button
                className="secondary-button"
                onClick={() => setShowCreateForm(true)}
                disabled={isLoading}
              >
                Create New Library
              </button>
            </div>
          </div>
        ) : (
          <div className="create-form">
            <div className="input-group">
              <label htmlFor="new-library-path">Library Location</label>
              <div className="input-with-button">
                <input
                  id="new-library-path"
                  type="text"
                  placeholder="Select a folder..."
                  value={libraryPath}
                  onChange={(e) => setLibraryPath(e.target.value)}
                  disabled={isLoading}
                  readOnly
                />
                <button
                  className="browse-button"
                  onClick={handleBrowseFolder}
                  disabled={isLoading}
                  type="button"
                >
                  Browse
                </button>
              </div>
            </div>

            <div className="input-group">
              <label htmlFor="library-name">Library Name</label>
              <input
                id="library-name"
                type="text"
                placeholder="My Video Library"
                value={newLibraryName}
                onChange={(e) => setNewLibraryName(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleCreateLibrary()}
                disabled={isLoading}
              />
            </div>

            <div className="button-group">
              <button
                className="primary-button"
                onClick={handleCreateLibrary}
                disabled={isLoading}
              >
                {isLoading ? 'Creating...' : 'Create Library'}
              </button>
              <button
                className="secondary-button"
                onClick={() => {
                  setShowCreateForm(false);
                  setLibraryPath('');
                  setNewLibraryName('');
                  setError(null);
                }}
                disabled={isLoading}
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        <div className="help-text">
          <p>Select an existing Dad Cam library folder or create a new one.</p>
          <p>Use the CLI to ingest footage: <code>dadcam ingest /path/to/source</code></p>
        </div>
      </div>
    </div>
  );
}

export default App;
