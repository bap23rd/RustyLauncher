import { useState, useEffect } from 'react';
import { Trash, PencilSimple, FolderOpen, Check, X, HardDrives, SlidersHorizontal } from '@phosphor-icons/react';
import { useNotification } from '../context/NotificationContext';
import { ServerConfigModal } from './ServerConfigModal';
import './ServerLibrary.css';

interface ServerLibraryProps {
    refreshKey: number;
    onRefresh: () => void;
}

export function ServerLibrary({ refreshKey, onRefresh }: ServerLibraryProps) {
    const notification = useNotification();
    const [servers, setServers] = useState<RustServer[]>([]);
    const [renamingId, setRenamingId] = useState<number | null>(null);
    const [newName, setNewName] = useState('');
    const [configuringServer, setConfiguringServer] = useState<RustServer | null>(null);

    useEffect(() => {
        loadServers();
    }, [refreshKey]);

    const loadServers = async () => {
        const list = await window.electronAPI.getServers();
        setServers(list);
    };

    const handleRename = (server: RustServer) => {
        setRenamingId(server.id);
        setNewName(server.name || '');
    };

    const submitRename = async (server: RustServer) => {
        if (!newName.trim()) {
            notification.show('Name cannot be empty', 'error');
            return;
        }
        const result = await window.electronAPI.renameServer(server.id, newName.trim());
        if (result.success) {
            setRenamingId(null);
            loadServers();
        } else {
            notification.show(`Failed to rename: ${result.error}`, 'error');
        }
    };

    const cancelRename = () => {
        setRenamingId(null);
        setNewName('');
    };

    const handleBrowseFiles = async (server: RustServer) => {
        await window.electronAPI.browseServerFiles(server.installPath);
    };

    const handleDelete = async (server: RustServer) => {
        if (!confirm(`Delete server "${server.name || server.id}"?\n\nThis will remove all files from disk.`)) {
            return;
        }
        const result = await window.electronAPI.deleteServer(server.id);
        if (result.success) {
            loadServers();
            onRefresh();
        } else {
            notification.show(`Failed to delete: ${result.error}`, 'error');
        }
    };

    if (servers.length === 0) {
        return (
            <div className="server-library-empty">
                <p>No servers installed yet.</p>
                <p>Click "Install Server" to get started!</p>
            </div>
        );
    }

    return (
        <div className="server-grid">
            {servers.map((server) => (
                <div key={server.id} className="server-card">
                    <div className="server-header">
                        {renamingId === server.id ? (
                            <div className="rename-input-container">
                                <input
                                    type="text"
                                    value={newName}
                                    onChange={(e) => setNewName(e.target.value)}
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter') submitRename(server);
                                        if (e.key === 'Escape') cancelRename();
                                    }}
                                    autoFocus
                                    className="rename-input"
                                />
                                <button className="btn-rename-confirm" onClick={() => submitRename(server)}><Check size={16} weight="bold" /></button>
                                <button className="btn-rename-cancel" onClick={cancelRename}><X size={16} weight="bold" /></button>
                            </div>
                        ) : (
                            <h3><HardDrives size={20} weight="bold" /> {server.name || `Server ${server.id}`}</h3>
                        )}
                        <button
                            className="icon-btn bin-btn"
                            onClick={() => handleDelete(server)}
                            title="Delete Server"
                        >
                            <Trash size={18} weight="bold" />
                        </button>
                    </div>

                    <div className="server-details">
                        <span className={`install-type-badge ${server.installType}`}>
                            {server.installType === 'latest' ? 'Latest Stable' : 'Pinned Manifest'}
                        </span>
                        {server.manifestId && <p><strong>Manifest:</strong> {server.manifestId}</p>}
                        <p className="server-path"><strong>Path:</strong> {server.installPath}</p>
                    </div>

                    <div className="server-actions">
                        <button className="btn-secondary-action" onClick={() => setConfiguringServer(server)}>
                            <SlidersHorizontal size={16} weight="bold" /> Configure Launch
                        </button>
                        <button className="btn-secondary-action" onClick={() => handleRename(server)}>
                            <PencilSimple size={16} weight="bold" /> Rename
                        </button>
                        <button className="btn-secondary-action" onClick={() => handleBrowseFiles(server)}>
                            <FolderOpen size={16} weight="bold" /> Browse Files
                        </button>
                    </div>
                </div>
            ))}

            {configuringServer && (
                <ServerConfigModal server={configuringServer} onClose={() => setConfiguringServer(null)} />
            )}
        </div>
    );
}
