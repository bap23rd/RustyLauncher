import { useState, useEffect, useRef } from 'react';
import type { CuratedVersion } from '../types/curatedVersions';
import './CuratedDownloadModal.css';

interface CuratedDownloadModalProps {
    version: CuratedVersion;
    onClose: () => void;
}

export function CuratedDownloadModal({ version, onClose }: CuratedDownloadModalProps) {
    const [versionName, setVersionName] = useState('');
    const [username, setUsername] = useState('');
    const [password, setPassword] = useState('');
    const [isDownloading, setIsDownloading] = useState(false);
    const [logs, setLogs] = useState<string[]>([]);
    const [installPath, setInstallPath] = useState('');
    const logsEndRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const removeProgressListener = window.electronAPI.onDownloadProgress((progress: { type: string; data: string }) => {
            setLogs((prev) => [...prev, progress.data]);
        });

        const removeCompleteListener = window.electronAPI.onDownloadComplete((result: { code: number | null }) => {
            setIsDownloading(false);
            if (result.code === 0) {
                setLogs((prev) => [...prev, '\n✅ Download completed successfully!']);
            } else {
                setLogs((prev) => [...prev, `\n❌ Download failed with code: ${result.code}`]);
            }
        });

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

        return () => {
            removeProgressListener();
            removeCompleteListener();
        };
    }, []);

    useEffect(() => {
        logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [logs]);

    const handleStartDownload = async () => {
        setIsDownloading(true);
        setLogs(['Starting download...\n']);

        await window.electronAPI.startDownload({
            clientManifestId: version.clientManifest,
            bundlesManifestId: version.bundleManifest,
            clientDepotId: '252495',
            bundlesDepotId: '252494',
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

    const handleCancel = () => {
        window.electronAPI.cancelDownload();
        setIsDownloading(false);
        setLogs((prev) => [...prev, '\n⚠️ Download cancelled by user']);
    };

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="curated-download-modal" onClick={(e) => e.stopPropagation()}>
                <div className="modal-header">
                    <h2>Download Version</h2>
                    <button className="close-btn" onClick={onClose}>×</button>
                </div>

                <div className="modal-content">
                    {!isDownloading ? (
                        <>
                            <div className="form-section">
                                <h3>Version Details</h3>
                                <div className="read-only-field">
                                    <label>Title</label>
                                    <div className="read-only-value">{version.title}</div>
                                </div>
                                <div className="read-only-field">
                                    <label>Client Manifest ID</label>
                                    <div className="read-only-value">{version.clientManifest}</div>
                                </div>
                                <div className="read-only-field">
                                    <label>Bundles Manifest ID</label>
                                    <div className="read-only-value">{version.bundleManifest}</div>
                                </div>
                                <div className="read-only-field">
                                    <label>Version Date</label>
                                    <div className="read-only-value">{version.versionDate}</div>
                                </div>
                            </div>

                            <div className="form-section">
                                <h3>Optional Settings</h3>
                                <div className="form-group">
                                    <label>Custom Name (Optional)</label>
                                    <input
                                        type="text"
                                        value={versionName}
                                        onChange={(e) => setVersionName(e.target.value)}
                                        placeholder="Leave empty for auto-generated name"
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
                                        />
                                        <button
                                            type="button"
                                            className="btn-browse"
                                            onClick={handleBrowsePath}
                                        >
                                            Browse
                                        </button>
                                    </div>
                                </div>
                            </div>

                            <div className="form-section">
                                <h3>Steam Credentials</h3>
                                <div className="form-group">
                                    <label>Steam Username</label>
                                    <input
                                        type="text"
                                        value={username}
                                        onChange={(e) => setUsername(e.target.value)}
                                        placeholder="Your Steam username"
                                    />
                                </div>
                                <div className="form-group">
                                    <label>Steam Password</label>
                                    <input
                                        type="password"
                                        value={password}
                                        onChange={(e) => setPassword(e.target.value)}
                                        placeholder="Your Steam password"
                                    />
                                </div>
                            </div>

                            <p className="settings-tip"><i>Tip: You can save your Steam login in Settings to auto-fill this.</i></p>

                            <div className="modal-actions">
                                <button className="btn-primary" onClick={handleStartDownload}>
                                    Start Download
                                </button>
                                <button className="btn-secondary" onClick={onClose}>
                                    Cancel
                                </button>
                            </div>
                        </>
                    ) : (
                        <>
                            <div className="terminal-view">
                                <div className="terminal-header">Download Progress</div>
                                <div className="terminal-content">
                                    {logs.map((log, index) => (
                                        <div key={index} className="log-line">{log}</div>
                                    ))}
                                    <div ref={logsEndRef} />
                                </div>
                            </div>
                            <div className="steam-guard-notice">
                                <p>⚠️ <strong>Important:</strong> You may be prompted by STEAM GUARD <b>once</b> at the start, and <b>again</b> a minute later.</p>
                                <p>Downloads can take 10-30 minutes (or even longer) depending on your connection speed.</p>
                            </div>
                            <div className="modal-actions">
                                <button className="btn-danger" onClick={handleCancel}>
                                    Cancel Download
                                </button>
                            </div>
                        </>
                    )}
                </div>
            </div>
        </div>
    );
}
