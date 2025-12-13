import { useState, useEffect, useRef } from 'react';
import type { OperationStatus } from '../App';
import { useNotification } from '../context/NotificationContext';
import './VersionLibrary.css';

interface RustVersion {
    id: number;
    name: string | null;
    manifestId: string;
    depotId: string;
    buildDate: string;
    installPath: string;
    isEacEnabled: number;
    lastPlayed: number;
}

interface VersionLibraryProps {
    onRefresh: () => void;
    activeOperations?: OperationStatus[];
    onOpenInstallPaths?: () => void;
}

export function VersionLibrary({ onRefresh, activeOperations = [], onOpenInstallPaths }: VersionLibraryProps) {
    const notification = useNotification();
    const [versions, setVersions] = useState<RustVersion[]>([]);
    const [eacAvailability, setEacAvailability] = useState<Record<number, boolean>>({});
    const [launching, setLaunching] = useState<number | null>(null);
    const [openMenuId, setOpenMenuId] = useState<number | null>(null);
    const [renamingId, setRenamingId] = useState<number | null>(null);
    const [newName, setNewName] = useState('');
    const menuRef = useRef<HTMLDivElement>(null);

    // Filter operations
    const unarchiving = activeOperations.filter(op => op.type === 'unarchive');
    const downloads = activeOperations.filter(op => op.type === 'download');

    useEffect(() => {
        loadVersions();

        // Close menu when clicking outside
        const handleClickOutside = (event: MouseEvent) => {
            if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
                setOpenMenuId(null);
            }
        };

        document.addEventListener('mousedown', handleClickOutside);
        return () => document.removeEventListener('mousedown', handleClickOutside);
    }, []);

    const loadVersions = async () => {
        // First, scan install paths for new versions and repair directories
        await window.electronAPI.scanInstallPaths();

        // Now load versions from database (may include newly discovered ones)
        const versionList = await window.electronAPI.getVersions();
        setVersions(versionList);

        // Check EAC availability for each version
        const availability: Record<number, boolean> = {};
        for (const version of versionList) {
            availability[version.id] = await window.electronAPI.checkEacClient(version.installPath);
        }
        setEacAvailability(availability);
    };

    const handleLaunch = async (version: RustVersion, eacEnabled: boolean) => {
        setLaunching(version.id);
        const result = await window.electronAPI.launchVersion(version.id, eacEnabled);
        setLaunching(null);

        if (!result.success) {
            notification.show(`Failed to launch: ${result.error}`, 'error');
        } else {
            loadVersions(); // Refresh to update lastPlayed
            onRefresh();
        }
    };

    const handleDelete = async (version: RustVersion) => {
        if (!confirm(`Delete version "${version.name || version.manifestId}"?\n\nThis will remove all files from disk.`)) {
            return;
        }

        const result = await window.electronAPI.deleteVersion(version.id);
        if (result.success) {
            loadVersions();
            onRefresh();
        } else {
            notification.show(`Failed to delete: ${result.error}`, 'error');
        }
    };

    const handleRename = async (version: RustVersion) => {
        setRenamingId(version.id);
        setNewName(version.name || '');
        setOpenMenuId(null);
    };

    const submitRename = async (version: RustVersion) => {
        if (!newName.trim()) {
            notification.show('Name cannot be empty', 'error');
            return;
        }

        const result = await window.electronAPI.renameVersion(version.id, newName.trim());
        if (result.success) {
            setRenamingId(null);
            loadVersions();
            onRefresh();
        } else {
            notification.show(`Failed to rename: ${result.error}`, 'error');
        }
    };

    const cancelRename = () => {
        setRenamingId(null);
        setNewName('');
    };

    const handleBrowseFiles = async (version: RustVersion) => {
        setOpenMenuId(null);
        await window.electronAPI.browseFiles(version.installPath);
    };

    const handleArchive = async (version: RustVersion) => {
        if (!confirm(`Archive version "${version.name || version.manifestId}"?\n\nThis will zip the version and move it to the archive folder, then remove it from the library.`)) {
            return;
        }

        setOpenMenuId(null);

        try {
            console.log('Starting archive for version:', version.id);
            // Fire and forget
            window.electronAPI.archiveVersion(version.id).catch(err => {
                console.error('Archive error:', err);
                notification.show(`Archive failed: ${err.message}`, 'error');
            });
        } catch (error: any) {
            console.error('Archive exception:', error);
        }
    };

    const isRecentlyPlayed = (version: RustVersion) => {
        if (versions.length === 0 || version.lastPlayed === 0) return false;
        const maxLastPlayed = Math.max(...versions.map(v => v.lastPlayed));
        return version.lastPlayed === maxLastPlayed && maxLastPlayed > 0;
    };

    if (versions.length === 0 && unarchiving.length === 0 && downloads.length === 0) {
        return (
            <div className="version-library-empty">
                <p>No versions downloaded yet.</p>
                <p>Click "Download Version" to get started!</p>
            </div>
        );
    }

    return (
        <div className="version-library">
            <div className="library-header">
                <h2>Version Library</h2>
                {onOpenInstallPaths && (
                    <button className="btn-add-location" onClick={onOpenInstallPaths}>
                        📁 Add Install Location
                    </button>
                )}
            </div>
            <div className="version-grid">
                {/* Download Placeholders */}
                {downloads.map(op => (
                    <div key={`download-${op.id}`} className="version-card placeholder downloading-placeholder">
                        <div className="status-overlay visible">
                            <div className="status-spinner"></div>
                            <div className="operation-status" style={{ fontSize: '2rem', fontWeight: 'bold', marginBottom: '1rem' }}>
                                {op.progress > 0 ? `${op.progress}%` : '0%'}
                            </div>
                            <div className="progress-bar-container" style={{ width: '80%', margin: '0 auto' }}>
                                <div
                                    className="progress-bar-fill"
                                    style={{ width: `${op.progress > 0 ? op.progress : 0}%` }}
                                ></div>
                            </div>
                            <button
                                className="btn-rename-cancel"
                                style={{ marginTop: '1rem', padding: '0.5rem 1rem', fontSize: '0.9rem' }}
                                onClick={() => window.electronAPI.cancelDownload()}
                            >
                                Cancel Download
                            </button>
                        </div>
                    </div>
                ))}

                {/* Unarchive Placeholders */}
                {unarchiving.map(op => (
                    <div key={`unarchive-${op.id}`} className="version-card placeholder unarchiving-placeholder">
                        <div className="version-header">
                            <h3>{op.id}</h3>
                            <span className="version-date">Restoring...</span>
                        </div>
                        <div className="version-actions">
                            <div className="status-overlay visible">
                                <div className="status-spinner"></div>
                                <div className="operation-status">{op.status}</div>
                                <div className="progress-bar-container">
                                    <div
                                        className="progress-bar-fill"
                                        style={{ width: `${op.progress}%` }}
                                    ></div>
                                </div>
                                <div className="operation-status">{op.progress}%</div>
                                {op.log && <div className="operation-log">{op.log}</div>}
                            </div>
                        </div>
                    </div>
                ))}

                {/* Existing Versions */}
                {versions.map((version) => {
                    const activeOp = activeOperations.find(op => op.type === 'archive' && op.id === version.id);
                    return (
                        <div key={version.id} className="version-card">
                            {activeOp && (
                                <div className="overlay-mask">
                                    <div className="progress-bar-container">
                                        <div className="progress-bar-fill" style={{ width: `${activeOp.progress}%` }}></div>
                                    </div>
                                    <div className="operation-status">{activeOp.status}</div>
                                    <div className="operation-status">{activeOp.progress}%</div>
                                </div>
                            )}
                            {isRecentlyPlayed(version) && (
                                <div className="recently-played-badge">Recently Played</div>
                            )}

                            <div className="version-header">
                                {renamingId === version.id ? (
                                    <div className="rename-input-container">
                                        <input
                                            type="text"
                                            value={newName}
                                            onChange={(e) => setNewName(e.target.value)}
                                            onKeyDown={(e) => {
                                                if (e.key === 'Enter') submitRename(version);
                                                if (e.key === 'Escape') cancelRename();
                                            }}
                                            autoFocus
                                            className="rename-input"
                                        />
                                        <button className="btn-rename-confirm" onClick={() => submitRename(version)}>✓</button>
                                        <button className="btn-rename-cancel" onClick={cancelRename}>✕</button>
                                    </div>
                                ) : (
                                    <h3>{version.name || `A Rust Version: ${version.id}`}</h3>
                                )}
                                <div className="version-icons">
                                    <button
                                        className="icon-btn cog-btn"
                                        onClick={() => setOpenMenuId(openMenuId === version.id ? null : version.id)}
                                        title="Extra Settings"
                                    >
                                        ⚙️
                                    </button>
                                    <button
                                        className="icon-btn bin-btn"
                                        onClick={() => handleDelete(version)}
                                        disabled={launching === version.id}
                                        title="Delete Version"
                                    >
                                        🗑️
                                    </button>
                                </div>
                            </div>

                            {openMenuId === version.id && (
                                <div className="extra-settings-menu" ref={menuRef}>
                                    <button onClick={() => handleRename(version)}>✏️ Change Name</button>
                                    <button onClick={() => handleBrowseFiles(version)}>📁 Browse Local Files</button>
                                    <button onClick={() => handleArchive(version)}>📦 Archive Version</button>
                                </div>
                            )}

                            <div className="version-details">
                                <p><strong>Manifest:</strong> {version.manifestId}</p>
                                <p><strong>Depot:</strong> {version.depotId}</p>
                                <p className="version-path"><strong>Path:</strong> {version.installPath}</p>
                                {eacAvailability[version.id] && (
                                    <span className="eac-badge">EAC Available</span>
                                )}
                            </div>

                            <div className="version-actions">
                                <button
                                    className="btn-launch"
                                    onClick={() => handleLaunch(version, true)}
                                    disabled={launching === version.id}
                                >
                                    {launching === version.id ? 'Launching...' : 'Launch (EAC)'}
                                </button>

                                {eacAvailability[version.id] && (
                                    <button
                                        className="btn-launch-no-eac"
                                        onClick={() => handleLaunch(version, false)}
                                        disabled={launching === version.id}
                                    >
                                        Launch (No EAC)
                                    </button>
                                )}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
