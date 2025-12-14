import { useState, useEffect } from 'react';
import { useNotification } from '../context/NotificationContext';
import './SettingsModal.css';

interface SettingsModalProps {
    onClose: () => void;
    initialSection?: SettingsSection;
}

type SettingsSection = 'general' | 'steam' | 'installPaths';

export function SettingsModal({ onClose, initialSection = 'general' }: SettingsModalProps) {
    const notification = useNotification();
    const [activeSection, setActiveSection] = useState<SettingsSection>(initialSection);
    const [steamUsername, setSteamUsername] = useState('');
    const [steamPassword, setSteamPassword] = useState('');
    const [isSaving, setIsSaving] = useState(false);
    const [diskUsage, setDiskUsage] = useState<{ libraryUsage: number; archiveUsage: number } | null>(null);
    const [installPaths, setInstallPaths] = useState<{ id: number; path: string; lastUsed: number }[]>([]);
    const [appVersion, setAppVersion] = useState<string>('...');

    useEffect(() => {
        loadSettings();
    }, []);

    const loadSettings = async () => {
        const settings = await window.electronAPI.getSettings();
        if (settings.steamUsername) {
            setSteamUsername(settings.steamUsername);
        }
        if (settings.steamPassword) {
            setSteamPassword(settings.steamPassword);
        }

        // Fetch disk usage
        window.electronAPI.getDiskUsage().then(usage => {
            setDiskUsage(usage);
        });

        // Fetch install paths
        loadInstallPaths();

        // Fetch app version
        window.electronAPI.getAppVersion().then(setAppVersion);
    };

    const loadInstallPaths = async () => {
        const paths = await window.electronAPI.getInstallPaths();
        setInstallPaths(paths);
    };

    const formatSize = (bytes: number) => {
        if (bytes === 0) return '0 GB';
        const gb = bytes / (1024 * 1024 * 1024);
        return `${gb.toFixed(2)} GB`;
    };

    const handleSaveSteamCredentials = async () => {
        setIsSaving(true);

        if (steamUsername.trim()) {
            await window.electronAPI.setSetting('steamUsername', steamUsername.trim());
        } else {
            await window.electronAPI.deleteSetting('steamUsername');
        }

        if (steamPassword.trim()) {
            await window.electronAPI.setSetting('steamPassword', steamPassword.trim());
        } else {
            await window.electronAPI.deleteSetting('steamPassword');
        }

        setIsSaving(false);
        notification.show('Steam credentials saved!', 'success');
    };

    const handleClearSteamCredentials = async () => {
        if (!confirm('Clear saved Steam credentials?')) return;

        await window.electronAPI.deleteSetting('steamUsername');
        await window.electronAPI.deleteSetting('steamPassword');
        setSteamUsername('');
        setSteamPassword('');
        setSteamUsername('');
        setSteamPassword('');
        notification.show('Steam credentials cleared!', 'success');
    };

    const handlePurgeAll = async () => {
        if (!confirm('⚠️ DANGER: Purge ALL versions?\n\nThis will permanently delete ALL downloaded versions from your disk.\n\nThis action CANNOT be undone!')) return;

        const result = await window.electronAPI.purgeAllVersions();
        if (result.success) {
            notification.show(`Successfully purged ${result.count} version(s)!`, 'success');
        } else {
            notification.show(`Failed to purge: ${result.error}`, 'error');
        }
    };

    const handleArchiveAll = async () => {
        if (!confirm('Archive ALL versions?\n\nThis will compress all versions into zip files and remove them from your library.\n\nThis may take several minutes.')) return;

        notification.show('Archiving all versions... This may take a while.', 'info');

        const result = await window.electronAPI.archiveAllVersions();
        if (result.success) {
            let message = `Successfully archived ${result.count} version(s)!`;
            if (result.errors && result.errors.length > 0) {
                message += `\n\nErrors:\n${result.errors.join('\n')}`;
            }
            notification.show(message, 'success');
        } else {
            notification.show(`Failed to archive: ${result.error}`, 'error');
        }
    };

    const handleAddInstallPath = async () => {
        const selectedPath = await window.electronAPI.browseDirectory();
        if (!selectedPath) return;

        const result = await window.electronAPI.addInstallPath(selectedPath);
        if (result.success) {
            notification.show('Install path added!', 'success');
            loadInstallPaths();
        } else {
            notification.show(result.error || 'Failed to add path', 'error');
        }
    };

    const handleRemoveInstallPath = async (id: number) => {
        if (installPaths.length <= 1) {
            notification.show('Cannot remove the last install path', 'error');
            return;
        }

        if (!confirm('Remove this install path?\n\nVersions in this location will no longer appear in your library.')) return;

        const result = await window.electronAPI.removeInstallPath(id);
        if (result.success) {
            notification.show('Install path removed!', 'success');
            loadInstallPaths();
        }
    };

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
                <div className="settings-header">
                    <h2>Settings</h2>
                    <button className="close-btn" onClick={onClose}>×</button>
                </div>

                <div className="settings-content">
                    <div className="settings-sidebar">
                        <button
                            className={activeSection === 'general' ? 'active' : ''}
                            onClick={() => setActiveSection('general')}
                        >
                            General
                        </button>
                        <button
                            className={activeSection === 'steam' ? 'active' : ''}
                            onClick={() => setActiveSection('steam')}
                        >
                            Steam Account
                        </button>
                        <button
                            className={activeSection === 'installPaths' ? 'active' : ''}
                            onClick={() => setActiveSection('installPaths')}
                        >
                            Install Paths
                        </button>
                    </div>

                    <div className="settings-main">
                        {activeSection === 'general' && (
                            <div className="settings-section">
                                <h3>General Settings</h3>

                                <div className="settings-group">
                                    <h4>About</h4>
                                    <div className="info-row">
                                        <span className="info-label">Version:</span>
                                        <span className="info-value">{appVersion}</span>
                                    </div>
                                    <div className="info-row">
                                        <span className="info-label">Library Size:</span>
                                        <span className="info-value">
                                            {diskUsage ? formatSize(diskUsage.libraryUsage) : 'Calculating...'}
                                        </span>
                                    </div>
                                    <div className="info-row">
                                        <span className="info-label">Archive Size:</span>
                                        <span className="info-value">
                                            {diskUsage ? formatSize(diskUsage.archiveUsage) : 'Calculating...'}
                                        </span>
                                    </div>
                                </div>

                                <div className="settings-group">
                                    <h4>Links</h4>
                                    <div style={{ display: 'flex', gap: '1rem', flexWrap: 'wrap' }}>
                                        <a
                                            href="https://github.com/bap23rd/RustyLauncher"
                                            target="_blank"
                                            rel="noreferrer"
                                            className="link-button"
                                        >
                                            🔗 RustyLauncher GitHub
                                        </a>
                                        <a
                                            href="#"
                                            target="_blank"
                                            rel="noreferrer"
                                            className="link-button"
                                            onClick={(e) => {
                                                // Prevent default navigation if it's just a placeholder
                                                if (e.currentTarget.getAttribute('href') === '#') {
                                                    e.preventDefault();
                                                    notification.show('Documentation link coming soon!', 'info');
                                                }
                                            }}
                                        >
                                            🔗 Support and Documentation
                                        </a>
                                    </div>
                                </div>

                                <div className="settings-group">
                                    <h4>Danger Zone</h4>
                                    <p className="warning-text">
                                        These actions cannot be undone. Use with caution.
                                    </p>
                                    <div className="danger-actions">
                                        <button className="btn-danger" onClick={handlePurgeAll}>
                                            🗑️ Purge All Versions
                                        </button>
                                        <button className="btn-warning" onClick={handleArchiveAll}>
                                            📦 Archive All Versions
                                        </button>
                                    </div>
                                </div>
                            </div>
                        )}

                        {activeSection === 'steam' && (
                            <div className="settings-section">
                                <h3>Steam Account</h3>

                                <div className="settings-group">
                                    <p className="info-text">
                                        Save your Steam credentials to automatically fill them when downloading versions.
                                        Your credentials are stored locally and never sent anywhere except to Steam's servers.
                                    </p>
                                </div>

                                <div className="settings-group">
                                    <label>Steam Username</label>
                                    <input
                                        type="text"
                                        value={steamUsername}
                                        onChange={(e) => setSteamUsername(e.target.value)}
                                        placeholder="Your Steam username"
                                    />
                                </div>

                                <div className="settings-group">
                                    <label>Steam Password</label>
                                    <input
                                        type="password"
                                        value={steamPassword}
                                        onChange={(e) => setSteamPassword(e.target.value)}
                                        placeholder="Your Steam password"
                                    />
                                </div>

                                <div className="settings-actions">
                                    <button
                                        className="btn-primary"
                                        onClick={handleSaveSteamCredentials}
                                        disabled={isSaving}
                                    >
                                        {isSaving ? 'Saving...' : 'Save Credentials'}
                                    </button>
                                    <button
                                        className="btn-secondary"
                                        onClick={handleClearSteamCredentials}
                                        disabled={isSaving || (!steamUsername && !steamPassword)}
                                    >
                                        Clear Credentials
                                    </button>
                                </div>
                            </div>
                        )}

                        {activeSection === 'installPaths' && (
                            <div className="settings-section">
                                <h3>Install Paths</h3>

                                <div className="settings-group">
                                    <p className="info-text">
                                        Manage directories where Rust versions are stored.
                                        The most recently used path will be auto-selected when downloading.
                                    </p>
                                </div>

                                <div className="settings-group">
                                    <h4>Saved Locations</h4>
                                    <div className="install-paths-list">
                                        {installPaths.length === 0 ? (
                                            <p className="empty-text">No install paths configured.</p>
                                        ) : (
                                            installPaths.map((pathItem, index) => (
                                                <div key={pathItem.id} className="install-path-item">
                                                    <div className="path-info">
                                                        <span className="path-text">{pathItem.path}</span>
                                                        {index === 0 && <span className="mru-badge">Most Recent</span>}
                                                    </div>
                                                    <button
                                                        className="btn-remove"
                                                        onClick={() => handleRemoveInstallPath(pathItem.id)}
                                                        title="Remove path"
                                                    >
                                                        ×
                                                    </button>
                                                </div>
                                            ))
                                        )}
                                    </div>
                                </div>

                                <div className="settings-actions">
                                    <button className="btn-primary" onClick={handleAddInstallPath}>
                                        📁 Add Install Location
                                    </button>
                                </div>
                            </div>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}
