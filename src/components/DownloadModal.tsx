import { useState, useEffect, useRef } from 'react';
import { useNotification } from '../context/NotificationContext';
import './DownloadModal.css';

interface DownloadModalProps {
    onClose: () => void;
}

export function DownloadModal({ onClose }: DownloadModalProps) {
    const notification = useNotification();
    const [versionName, setVersionName] = useState('');
    const [clientManifestId, setClientManifestId] = useState('');
    const [bundlesManifestId, setBundlesManifestId] = useState('');
    const [clientDepotId, setClientDepotId] = useState('252495');
    const [bundlesDepotId, setBundlesDepotId] = useState('252494');
    const [showAdvanced, setShowAdvanced] = useState(false);
    const [username, setUsername] = useState('');
    const [password, setPassword] = useState('');
    const [isDownloading, setIsDownloading] = useState(false);
    const [logs, setLogs] = useState<string[]>([]);
    const [installPath, setInstallPath] = useState('');
    const logsEndRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const handleProgress = (progress: { type: string; data: string }) => {
            setLogs((prev) => [...prev, progress.data]);
        };

        const handleComplete = (result: { code: number | null }) => {
            setIsDownloading(false);
            if (result.code === 0) {
                setLogs((prev) => [...prev, '\n✅ Download completed successfully!']);
            } else {
                setLogs((prev) => [...prev, `\n❌ Download failed with code: ${result.code}`]);
            }
        };

        window.electronAPI.onDownloadProgress(handleProgress);
        window.electronAPI.onDownloadComplete(handleComplete);

        // Load saved Steam credentials
        const loadSettings = async () => {
            const settings = await window.electronAPI.getSettings();
            if (settings.steamUsername) {
                setUsername(settings.steamUsername);
            }
            if (settings.steamPassword) {
                setPassword(settings.steamPassword);
            }
        };
        loadSettings();

        // Load most recently used install path
        window.electronAPI.getMostRecentPath().then(path => {
            if (path) setInstallPath(path);
        });
    }, []);

    useEffect(() => {
        logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [logs]);

    const handleStartDownload = async () => {
        if (!clientManifestId.trim()) {
            notification.show('Please enter a Client Manifest ID', 'error');
            return;
        }
        if (!bundlesManifestId.trim()) {
            notification.show('Please enter a Bundles Manifest ID', 'error');
            return;
        }

        setIsDownloading(true);
        setLogs(['Starting dual-depot download...\n']);

        await window.electronAPI.startDownload({
            clientManifestId,
            bundlesManifestId,
            clientDepotId,
            bundlesDepotId,
            username: username || undefined,
            password: password || undefined,
            name: versionName.trim() || undefined,
            installPath: installPath || undefined,
        });
    };

    const handleBrowsePath = async () => {
        const selectedPath = await window.electronAPI.browseDirectory();
        if (selectedPath) {
            setInstallPath(selectedPath);
        }
    };

    const handleCancel = async () => {
        await window.electronAPI.cancelDownload();
        setIsDownloading(false);
        setLogs((prev) => [...prev, '\n⚠️ Download cancelled by user.']);
    };

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                <div className="modal-header">
                    <h2>Download Rust Version</h2>
                    <button className="close-btn" onClick={onClose}>×</button>
                </div>

                <div className="modal-body">
                    <div className="form-group">
                        <label>Version Name (Optional)</label>
                        <input
                            type="text"
                            value={versionName}
                            onChange={(e) => setVersionName(e.target.value)}
                            placeholder="e.g., Jungle Update or leave blank for auto-naming"
                            disabled={isDownloading}
                        />
                        <small>If left blank, will be named "A Rust Version: 1", "A Rust Version: 2", etc.</small>
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
                            <button
                                type="button"
                                className="btn-browse"
                                onClick={handleBrowsePath}
                                disabled={isDownloading}
                            >
                                Browse
                            </button>
                        </div>
                        <small>Where the version will be downloaded to</small>
                    </div>

                    <div className="form-group">
                        <label>Client Manifest ID *</label>
                        <input
                            type="text"
                            value={clientManifestId}
                            onChange={(e) => setClientManifestId(e.target.value)}
                            placeholder="Enter client manifest ID"
                            disabled={isDownloading}
                        />
                        <small>
                            Find Client Manifest IDs at{' '}
                            <a href="https://steamdb.info/depot/252495/manifests/" target="_blank" rel="noreferrer">
                                SteamDB
                            </a>
                        </small>
                    </div>

                    <div className="form-group">
                        <label>Bundles Manifest ID *</label>
                        <input
                            type="text"
                            value={bundlesManifestId}
                            onChange={(e) => setBundlesManifestId(e.target.value)}
                            placeholder="Enter bundles manifest ID"
                            disabled={isDownloading}
                        />
                        <small>
                            Find Bundles Manifest IDs at{' '}
                            <a href="https://steamdb.info/depot/252494/manifests/" target="_blank" rel="noreferrer">
                                SteamDB
                            </a>
                        </small>
                    </div>

                    <div className="advanced-toggle">
                        <button
                            type="button"
                            onClick={() => setShowAdvanced(!showAdvanced)}
                            disabled={isDownloading}
                        >
                            {showAdvanced ? '▼' : '▶'} Advanced Settings
                        </button>
                    </div>

                    {showAdvanced && (
                        <div className="advanced-settings">
                            <div className="form-group">
                                <label>Client Depot ID</label>
                                <input
                                    type="text"
                                    value={clientDepotId}
                                    onChange={(e) => setClientDepotId(e.target.value)}
                                    disabled={isDownloading}
                                />
                            </div>

                            <div className="form-group">
                                <label>Bundles Depot ID</label>
                                <input
                                    type="text"
                                    value={bundlesDepotId}
                                    onChange={(e) => setBundlesDepotId(e.target.value)}
                                    disabled={isDownloading}
                                />
                            </div>
                        </div>
                    )}

                    <div className="form-group">
                        <label>Steam Username (Required)</label>
                        <input
                            type="text"
                            value={username}
                            onChange={(e) => setUsername(e.target.value)}
                            placeholder="Your Steam username"
                            disabled={isDownloading}
                        />
                    </div>

                    <div className="form-group">
                        <label>Steam Password (Required)</label>
                        <input
                            type="password"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            placeholder="Your Steam password"
                            disabled={isDownloading}
                        />
                        <small className="steam-login-tip">
                            Tip: You can save your Steam login in Settings to auto-fill this.
                        </small>
                    </div>

                    {logs.length > 0 && (
                        <div className="logs-container">
                            <h3>Download Progress</h3>
                            <div className="logs">
                                {logs.map((log, index) => (
                                    <div key={index} className="log-line">{log}</div>
                                ))}
                                <div ref={logsEndRef} />
                            </div>
                            <p className="download-disclaimer">
                                This will take several minutes.<br />
                                If you use <b>STEAM GUARD</b> you will be prompted once, then again a minute later.
                            </p>
                        </div>
                    )}

                    <div className="modal-actions">
                        {!isDownloading ? (
                            <button className="btn-primary" onClick={handleStartDownload}>
                                Start Download
                            </button>
                        ) : (
                            <button className="btn-danger" onClick={handleCancel}>
                                Cancel Download
                            </button>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}
