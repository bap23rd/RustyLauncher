import { useState, useEffect } from 'react';
import { useNotification } from '../context/NotificationContext';
import './UnarchiveModal.css';

interface UnarchiveModalProps {
    onClose: () => void;
    onStartUnarchive: (name: string) => void;
}

interface ArchivedVersion {
    filename: string;
    name: string;
    path: string;
    size: number;
}

export function UnarchiveModal({ onClose, onStartUnarchive }: UnarchiveModalProps) {
    const notification = useNotification();
    const [archives, setArchives] = useState<ArchivedVersion[]>([]);
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        loadArchives();
    }, []);

    const loadArchives = async () => {
        setLoading(true);
        try {
            const archiveList = await window.electronAPI.listArchives();
            setArchives(archiveList);
        } catch (err) {
            console.error(err);
            notification.show('Failed to load archives', 'error');
        } finally {
            setLoading(false);
        }
    };

    const handleUnarchive = (archive: ArchivedVersion) => {
        if (!confirm(`Restore "${archive.name}"?\n\nThis will extract the archive and add it back to your library.`)) {
            return;
        }

        // Fire and forget - progress tracked via events
        onStartUnarchive(archive.name);

        window.electronAPI.unarchiveVersion(archive.path, archive.name)
            .catch(err => {
                notification.show(`Failed to start restore: ${err.message}`, 'error');
            });

        onClose();
    };

    const formatFileSize = (bytes: number): string => {
        if (bytes < 1024) return bytes + ' B';
        if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(2) + ' KB';
        if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(2) + ' MB';
        return (bytes / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
    };

    return (
        <div className="modal-overlay" onClick={onClose}>
            <div className="unarchive-modal" onClick={(e) => e.stopPropagation()}>
                <div className="modal-header">
                    <h2>📦 Archived Versions</h2>
                    <button className="close-btn" onClick={onClose}>×</button>
                </div>

                <div className="modal-content">
                    {loading ? (
                        <div className="loading-state">
                            <p>Loading archives...</p>
                        </div>
                    ) : archives.length === 0 ? (
                        <div className="empty-state">
                            <p>No archived versions found.</p>
                            <p className="empty-hint">Archive a version to see it here.</p>
                        </div>
                    ) : (
                        <div className="archives-list">
                            {archives.map((archive) => (
                                <div key={archive.filename} className="archive-item">
                                    <div className="archive-info">
                                        <h3>{archive.name}</h3>
                                        <p className="archive-size">{formatFileSize(archive.size)}</p>
                                    </div>
                                    <button
                                        className="btn-unarchive"
                                        onClick={() => handleUnarchive(archive)}
                                    >
                                        ↩️ Restore
                                    </button>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
