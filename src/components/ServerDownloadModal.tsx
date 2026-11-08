import { useState, useEffect, useRef } from 'react';
import { useNotification } from '../context/NotificationContext';
import '../components/DownloadModal.css';
import './ServerDownloadModal.css';

interface ServerDownloadModalProps {
    onClose: () => void;
}

type InstallMode = 'latest' | 'manifest';

export function ServerDownloadModal({ onClose }: ServerDownloadModalProps) {
    const notification = useNotification();
    const [mode, setMode] = useState<InstallMode>('latest');
    const [serverName, setServerName] = useState('');
    const appId = '258550'; // Rust Dedicated Server
    const [depotId, setDepotId] = useState('');
    const [manifestId, setManifestId] = useState('');
    const [username, setUsername] = useState('');
    const [password, setPassword] = useState('');
    const [installPath, setInstallPath] = useState('');
    const [isDownloading, setIsDownloading] = useState(false);
    const [logs, setLogs] = useState<string[]>([]);
    const logsEndRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const handleProgress = (progress: { type: string; data: string }) => {
            setLogs((prev) => [...prev, progress.data]);
        };

        const handleComplete = (result: { code: number | null }) => {
            setIsDownloading(false);
            if (result.code === 0) {
                setLogs((prev) => [...prev, '\nServer install completed successfully!']);
            } else {
                setLogs((prev) => [...prev, `\nServer install failed with code: ${result.code}`]);
            }
        };

        const removeProgress = window.electronAPI.onServerDownloadProgress(handleProgress);
        const removeComplete = window.electronAPI.onServerDownloadComplete(handleComplete);

        window.electronAPI.getMostRecentPath().then(path => {
            if (path) setInstallPath(path);
        });

        return () => {
            removeProgress();
            removeComplete();
        };
    }, []);

    useEffect(() => {
        logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [logs]);

    const handleBrowsePath = async () => {
        const selectedPath = await window.electronAPI.browseDirectory();
        if (selectedPath) setInstallPath(selectedPath);
    };

    const handleStart = async () => {
        if (mode === 'manifest' && (!depotId.trim() || !manifestId.trim())) {
            notification.show('Please enter both a Depot ID and Manifest ID', 'error');
            return;
        }

        setIsDownloading(true);
        setLogs([mode === 'latest' ? 'Installing latest stable server build via SteamCMD...\n' : 'Installing pinned server build via DepotDownloader...\n']);

        if (mode === 'latest') {
            await window.electronAPI.startServerDownloadLatest({
                name: serverName.trim() || undefined,
                appId,
                installPath: installPath || undefined,
            });
        } else {
            await window.electronAPI.startServerDownloadManifest({
                name: serverName.trim() || undefined,
                appId,
                depotId,
                manifestId,
                username: username || undefined,
                password: password || undefined,
                installPath: installPath || undefined,
            });
        }
    };

    const handleCancel = async () => {
        await window.electronAPI.cancelServerDownload();
        setIsDownloading(false);
        setLogs((prev) => [...prev, '\nInstall cancelled by user.']);
    };

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                <div className="modal-header">
                    <h2>Install Rust Server</h2>
                    <button className="close-btn" onClick={onClose}>×</button>
                </div>

                <div className="modal-body">
                    <div className="server-mode-toggle">
                        <button
                            type="button"
                            className={mode === 'latest' ? 'active' : ''}
                            onClick={() => setMode('latest')}
                            disabled={isDownloading}
                        >
                            Latest Stable
                        </button>
                        <button
                            type="button"
                            className={mode === 'manifest' ? 'active' : ''}
                            onClick={() => setMode('manifest')}
                            disabled={isDownloading}
                        >
                            Specific Version (Manifest)
                        </button>
                    </div>

                    <div className="form-group">
                        <label>Server Name (Optional)</label>
                        <input
                            type="text"
                            value={serverName}
                            onChange={(e) => setServerName(e.target.value)}
                            placeholder="e.g., My Permanent Server"
                            disabled={isDownloading}
                        />
                    </div>

                    <div className="form-group">
                        <label>Install Location</label>
                        <div className="path-input-group">
                            <input
                                type="text"
                                value={installPath}
                                onChange={(e) => setInstallPath(e.target.value)}
                                placeholder="Select install directory"
                                disabled={isDownloading}
                            />
                            <button type="button" className="btn-browse" onClick={handleBrowsePath} disabled={isDownloading}>
                                Browse
                            </button>
                        </div>
                    </div>

                    {mode === 'latest' ? (
                        <p className="server-mode-note">
                            Installs the current public branch of the Rust Dedicated Server via SteamCMD (anonymous login — no Steam account needed). Always tracks whatever Facepunch currently ships.
                        </p>
                    ) : (
                        <>
                            <div className="form-group">
                                <label>Depot ID *</label>
                                <input
                                    type="text"
                                    value={depotId}
                                    onChange={(e) => setDepotId(e.target.value)}
                                    placeholder="Enter server depot ID"
                                    disabled={isDownloading}
                                />
                            </div>
                            <div className="form-group">
                                <label>Manifest ID *</label>
                                <input
                                    type="text"
                                    value={manifestId}
                                    onChange={(e) => setManifestId(e.target.value)}
                                    placeholder="Enter manifest ID"
                                    disabled={isDownloading}
                                />
                                <small>
                                    Find Manifest IDs at{' '}
                                    <a href="https://steamdb.info/app/258550/depots/" target="_blank" rel="noreferrer">
                                        SteamDB
                                    </a>
                                </small>
                            </div>
                            <div className="form-group">
                                <label>Steam Username (Optional)</label>
                                <input
                                    type="text"
                                    value={username}
                                    onChange={(e) => setUsername(e.target.value)}
                                    placeholder="Required only for non-public manifests"
                                    disabled={isDownloading}
                                />
                            </div>
                            <div className="form-group">
                                <label>Steam Password (Optional)</label>
                                <input
                                    type="password"
                                    value={password}
                                    onChange={(e) => setPassword(e.target.value)}
                                    disabled={isDownloading}
                                />
                            </div>
                        </>
                    )}

                    {logs.length > 0 && (
                        <div className="logs-container">
                            <h3>Install Progress</h3>
                            <div className="logs">
                                {logs.map((log, index) => (
                                    <div key={index} className="log-line">{log}</div>
                                ))}
                                <div ref={logsEndRef} />
                            </div>
                        </div>
                    )}

                    <div className="modal-actions">
                        {!isDownloading ? (
                            <button className="btn-primary" onClick={handleStart}>
                                Start Install
                            </button>
                        ) : (
                            <button className="btn-danger" onClick={handleCancel}>
                                Cancel Install
                            </button>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}
