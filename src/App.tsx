import { useState, useEffect } from 'react';
import './App.css';
import { DownloadModal } from './components/DownloadModal';
import { CuratedDownloadModal } from './components/CuratedDownloadModal';
import { SettingsModal } from './components/SettingsModal';
import { StickyFooter } from './components/StickyFooter';
import { UnarchiveModal } from './components/UnarchiveModal';
import { Sidebar } from './components/Sidebar';
import { AtmosphereBackground } from './components/AtmosphereBackground';
import { ServerDownloadModal } from './components/ServerDownloadModal';
import { HomePage } from './pages/HomePage';
import { LibraryPage } from './pages/LibraryPage';
import { DownloadPage } from './pages/DownloadPage';
import { ServersPage } from './pages/ServersPage';
import type { CuratedVersion } from './types/curatedVersions';

type Page = 'home' | 'library' | 'downloads' | 'servers';

export interface OperationStatus {
    type: 'archive' | 'unarchive' | 'download';
    id: string | number; // versionId for archive, name for unarchive/download
    progress: number;
    status: string;
    log?: string;
}

function App() {
    const [currentPage, setCurrentPage] = useState<Page>('home');
    const [showDownloadModal, setShowDownloadModal] = useState(false);
    const [showServerDownloadModal, setShowServerDownloadModal] = useState(false);
    const [serversRefreshKey, setServersRefreshKey] = useState(0);
    const [showSettingsModal, setShowSettingsModal] = useState(false);
    const [showUnarchiveModal, setShowUnarchiveModal] = useState(false);
    const [selectedCuratedVersion, setSelectedCuratedVersion] = useState<CuratedVersion | null>(null);
    const [refreshKey, setRefreshKey] = useState(0);
    const [activeOperations, setActiveOperations] = useState<OperationStatus[]>([]);
    const [settingsInitialSection, setSettingsInitialSection] = useState<'general' | 'steam' | 'installPaths'>('general');

    const openInstallPathsSettings = () => {
        setSettingsInitialSection('installPaths');
        setShowSettingsModal(true);
    };

    const handleOpenSettings = () => {
        setSettingsInitialSection('general');
        setShowSettingsModal(true);
    };

    const handleRefresh = () => {
        setRefreshKey(prev => prev + 1);
    };

    const updateOperation = (type: 'archive' | 'unarchive' | 'download', id: string | number, progress: number, status: string) => {
        setActiveOperations(prev => {
            const existing = prev.find(op => op.type === type && op.id === id);

            // If progress is -1, preserve existing progress (or 0 if new)
            // If progress is -1 and no existing, default to 0.
            const newProgress = (progress === -1) ? (existing ? existing.progress : 0) : progress;

            if (newProgress >= 100) {
                // Delay removal to prevent flicker (for archive) or ensure completion is seen
                setTimeout(() => {
                    handleRefresh();
                    setActiveOperations(current => current.filter(op => !(op.type === type && op.id === id)));
                }, 1000); // 1 second delay

                // Keep it at 100% for now
                if (existing) {
                    return prev.map(op => (op.type === type && op.id === id) ? { ...op, progress: 100, status: 'Completed', log: 'Completed' } : op);
                }
                return [...prev, { type, id, progress: 100, status: 'Completed', log: 'Completed' }];
            }

            if (existing) {
                return prev.map(op => (op.type === type && op.id === id) ? { ...op, progress: newProgress, status, log: status } : op);
            }
            return [...prev, { type, id, progress: newProgress, status, log: status }];
        });
    };

    // Expose a method to manually start an operation state (for instant feedback)
    const startOperation = (type: 'archive' | 'unarchive' | 'download', id: string | number, status: string) => {
        setActiveOperations(prev => {
            // Check if it exists.
            if (prev.find(op => op.type === type && op.id === id)) return prev;
            return [...prev, { type, id, progress: 0, status, log: status }];
        });
    };

    useEffect(() => {
        const removeArchiveListener = window.electronAPI.onArchiveProgress((data) => {
            updateOperation('archive', data.id, data.progress, data.status);
        });

        const removeUnarchiveListener = window.electronAPI.onUnarchiveProgress((data) => {
            updateOperation('unarchive', data.name, data.progress, data.log || '');
        });

        // Track downloads
        const removeDownloadProgressListener = window.electronAPI.onDownloadProgress((data: any) => {
            // Try to extract percentage from log line
            const match = data.data && typeof data.data === 'string' && data.data.match(/(\d+)%/);
            const p = match ? parseInt(match[1]) : -1;

            // We use generic status "Downloading..." so UI can handle display logic (e.g. showing % instead of text)
            // This prevents rapid flashing of log lines in the UI.
            updateOperation('download', data.name || 'Unknown', p, 'Downloading...');
        });

        const removeDownloadCompleteListener = window.electronAPI.onDownloadComplete((data: any) => {
            if (data.code === 0) {
                updateOperation('download', data.name || 'Unknown', 100, 'Completed');
            } else {
                // On failure
                updateOperation('download', data.name || 'Unknown', 100, 'Failed');
                // Remove after delay
                setTimeout(() => {
                    setActiveOperations(current => current.filter(op => !(op.type === 'download' && op.id === (data.name || 'Unknown'))));
                }, 4000);
            }
        });

        return () => {
            removeArchiveListener();
            removeUnarchiveListener();
            if (removeDownloadProgressListener) removeDownloadProgressListener();
            if (removeDownloadCompleteListener) removeDownloadCompleteListener();
        };
    }, []);

    const handleDownloadClose = () => {
        setShowDownloadModal(false);
        setRefreshKey(prev => prev + 1); // Refresh version library
    };

    const handleServerDownloadClose = () => {
        setShowServerDownloadModal(false);
        setServersRefreshKey(prev => prev + 1); // Refresh server library
    };

    const handleNavigate = (page: Page) => {
        setCurrentPage(page);
    };

    return (
        <div className="app-shell">
            <Sidebar currentPage={currentPage} onNavigate={handleNavigate} onOpenSettings={handleOpenSettings} />

            <div className="app-container">
                <AtmosphereBackground />

                <div className="content-wrapper">
                    {currentPage === 'library' && (
                        <header className="page-header">
                            <h2>Library</h2>
                            <button className="unarchive-btn" onClick={() => setShowUnarchiveModal(true)}>
                                Unarchive
                            </button>
                        </header>
                    )}
                    {currentPage === 'downloads' && (
                        <header className="page-header">
                            <h2>Download a Version</h2>
                            <button className="download-btn" onClick={() => setShowDownloadModal(true)}>
                                Custom Download
                            </button>
                        </header>
                    )}
                    {currentPage === 'servers' && (
                        <header className="page-header">
                            <h2>Servers</h2>
                            <button className="download-btn" onClick={() => setShowServerDownloadModal(true)}>
                                Install Server
                            </button>
                        </header>
                    )}

                    <main className="main-content">
                        {currentPage === 'home' && (
                            <HomePage
                                onNavigateLibrary={() => setCurrentPage('library')}
                                onNavigateDownloads={() => setCurrentPage('downloads')}
                                onNavigateServers={() => setCurrentPage('servers')}
                            />
                        )}
                        {currentPage === 'library' && (
                            <LibraryPage
                                onRefresh={handleRefresh}
                                refreshKey={refreshKey}
                                activeOperations={activeOperations}
                                onOpenInstallPaths={openInstallPathsSettings}
                            />
                        )}
                        {currentPage === 'downloads' && (
                            <DownloadPage
                                onOpenCuratedDownload={(version) => setSelectedCuratedVersion(version)}
                            />
                        )}
                        {currentPage === 'servers' && (
                            <ServersPage
                                refreshKey={serversRefreshKey}
                                onRefresh={() => setServersRefreshKey(prev => prev + 1)}
                            />
                        )}
                    </main>
                </div>
            </div>

            <StickyFooter
                onNavigate={handleNavigate}
                onOpenSettings={handleOpenSettings}
                onOpenDownload={() => setShowDownloadModal(true)}
                activeOperations={activeOperations}
            />

            {showDownloadModal && <DownloadModal onClose={handleDownloadClose} />}
            {showServerDownloadModal && <ServerDownloadModal onClose={handleServerDownloadClose} />}
            {showSettingsModal && <SettingsModal onClose={() => setShowSettingsModal(false)} initialSection={settingsInitialSection} />}
            {showUnarchiveModal && (
                <UnarchiveModal
                    onClose={() => setShowUnarchiveModal(false)}
                    onStartUnarchive={(name) => startOperation('unarchive', name, 'Starting...')}
                />
            )}
            {selectedCuratedVersion && (
                <CuratedDownloadModal
                    version={selectedCuratedVersion}
                    onClose={() => setSelectedCuratedVersion(null)}
                />
            )}
        </div>
    );
}

export default App;
