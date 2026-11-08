import { useState, useEffect } from 'react';
import { Folders } from '@phosphor-icons/react';
import { useNotification } from '../context/NotificationContext';
import '../components/DownloadModal.css';
import './ServerConfigModal.css';

interface ServerConfigModalProps {
    server: RustServer;
    onClose: () => void;
}

const FIELD_LABELS: { key: keyof Omit<ServerLaunchConfig, 'customArgs' | 'rconWeb'>; label: string; placeholder: string }[] = [
    { key: 'hostname', label: 'Server Hostname', placeholder: 'My Rust Server' },
    { key: 'identity', label: 'Identity (save folder name)', placeholder: 'rustyserver' },
    { key: 'port', label: 'Server Port', placeholder: '28015' },
    { key: 'rconPort', label: 'RCON Port', placeholder: '28016' },
    { key: 'rconPassword', label: 'RCON Password', placeholder: 'changeme' },
    { key: 'maxPlayers', label: 'Max Players', placeholder: '50' },
    { key: 'worldSize', label: 'World Size', placeholder: '3000' },
    { key: 'seed', label: 'Map Seed', placeholder: '12345' },
    { key: 'saveInterval', label: 'Save Interval (seconds)', placeholder: '300' },
];

export function ServerConfigModal({ server, onClose }: ServerConfigModalProps) {
    const notification = useNotification();
    const [config, setConfig] = useState<ServerLaunchConfig | null>(null);
    const [hasOxide, setHasOxide] = useState(false);
    const [isSaving, setIsSaving] = useState(false);

    useEffect(() => {
        window.electronAPI.getServerConfig(server.id).then((result) => {
            if (result) {
                setConfig(result.config);
                setHasOxide(result.hasOxide);
            }
        });
    }, [server.id]);

    const updateField = (key: keyof ServerLaunchConfig, patch: Partial<ServerLaunchArg>) => {
        if (!config) return;
        setConfig({ ...config, [key]: { ...(config[key] as ServerLaunchArg), ...patch } });
    };

    const handleSave = async () => {
        if (!config) return;
        setIsSaving(true);
        const result = await window.electronAPI.saveServerConfig(server.id, config);
        setIsSaving(false);
        if (result.success) {
            notification.show('Server launch config saved! RustServer.bat has been updated.', 'success');
            onClose();
        } else {
            notification.show(`Failed to save: ${result.error}`, 'error');
        }
    };

    const handleOpenMods = async () => {
        await window.electronAPI.openServerModsFolder(server.id);
    };

    if (!config) {
        return (
            <div className="modal-overlay" onClick={onClose}>
                <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                    <div className="modal-body">Loading...</div>
                </div>
            </div>
        );
    }

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                <div className="modal-header">
                    <h2>Configure Launch — {server.name}</h2>
                    <button className="close-btn" onClick={onClose}>×</button>
                </div>

                <div className="modal-body">
                    <p className="server-config-intro">
                        Toggle the arguments you want written into <code>RustServer.bat</code>. Unchecked fields are left out entirely.
                    </p>

                    {FIELD_LABELS.map(({ key, label, placeholder }) => {
                        const field = config[key] as ServerLaunchArg;
                        return (
                            <div className="server-config-row" key={key}>
                                <label className="server-config-checkbox">
                                    <input
                                        type="checkbox"
                                        checked={field.enabled}
                                        onChange={(e) => updateField(key, { enabled: e.target.checked })}
                                    />
                                    {label}
                                </label>
                                <input
                                    type={key === 'rconPassword' ? 'password' : 'text'}
                                    value={field.value}
                                    onChange={(e) => updateField(key, { value: e.target.value })}
                                    placeholder={placeholder}
                                    disabled={!field.enabled}
                                />
                            </div>
                        );
                    })}

                    <div className="server-config-row">
                        <label className="server-config-checkbox">
                            <input
                                type="checkbox"
                                checked={config.rconWeb.enabled}
                                onChange={(e) => setConfig({ ...config, rconWeb: { enabled: e.target.checked } })}
                            />
                            Enable RCON Web Interface
                        </label>
                    </div>

                    <div className="form-group">
                        <label>Custom Arguments</label>
                        <input
                            type="text"
                            value={config.customArgs}
                            onChange={(e) => setConfig({ ...config, customArgs: e.target.value })}
                            placeholder="e.g. +server.tickrate 30 +server.pve true"
                        />
                        <small>Appended to the end of the launch command, space-separated.</small>
                    </div>

                    <div className="server-config-mods">
                        <div>
                            <strong>Mods</strong>
                            <p>
                                {hasOxide ? 'Oxide detected — no special launch arguments are needed for it to load.' : 'No mod loader detected yet.'}
                                {' '}Get Oxide at{' '}
                                <a href="https://umod.org/games/rust" target="_blank" rel="noreferrer">umod.org</a>.
                            </p>
                        </div>
                        <button type="button" className="btn-browse" onClick={handleOpenMods}>
                            <Folders size={16} weight="bold" /> Mods Folder
                        </button>
                    </div>

                    <div className="modal-actions">
                        <button className="btn-primary" onClick={handleSave} disabled={isSaving}>
                            {isSaving ? 'Saving...' : 'Save & Update .bat'}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
