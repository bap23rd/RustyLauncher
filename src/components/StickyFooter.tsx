import { useState, useEffect, useRef } from 'react';
import type { OperationStatus } from '../App';
import { useNotification } from '../context/NotificationContext';
import './StickyFooter.css';

interface StickyFooterProps {
    onNavigate: (page: 'home' | 'library') => void;
    onOpenSettings: () => void;
    onOpenDownload: () => void;
    activeOperations?: OperationStatus[];
}

export function StickyFooter({ onNavigate, onOpenSettings, onOpenDownload, activeOperations = [] }: StickyFooterProps) {
    const notification = useNotification();
    const [lastPlayedVersion, setLastPlayedVersion] = useState<any>(null);
    const [lastLaunchMode, setLastLaunchMode] = useState<'eac' | 'no-eac'>('eac');
    const [showMenu, setShowMenu] = useState(false);
    const [launching, setLaunching] = useState(false);
    const menuRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        loadLastPlayed();

        // Close menu when clicking outside
        const handleClickOutside = (event: MouseEvent) => {
            if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
                setShowMenu(false);
            }
        };

        document.addEventListener('mousedown', handleClickOutside);
        return () => document.removeEventListener('mousedown', handleClickOutside);
    }, []);

    const loadLastPlayed = async () => {
        const version = await window.electronAPI.getLastPlayedVersion();
        setLastPlayedVersion(version);

        const settings = await window.electronAPI.getSettings();
        if (settings.lastLaunchMode) {
            setLastLaunchMode(settings.lastLaunchMode as 'eac' | 'no-eac');
        }
    };

    const handleQuickLaunch = async () => {
        if (!lastPlayedVersion || launching) return;

        setLaunching(true);
        const eacEnabled = lastLaunchMode === 'eac';
        const result = await window.electronAPI.launchVersion(lastPlayedVersion.id, eacEnabled);
        setLaunching(false);

        if (!result.success) {
            notification.show(`Failed to launch: ${result.error}`, 'error');
        } else {
            // Reload to update last played
            loadLastPlayed();
        }
    };

    const handleAlternateLaunch = async () => {
        if (!lastPlayedVersion || launching) return;

        setShowMenu(false);
        setLaunching(true);
        const eacEnabled = lastLaunchMode === 'no-eac'; // Opposite mode
        const result = await window.electronAPI.launchVersion(lastPlayedVersion.id, eacEnabled);
        setLaunching(false);

        if (!result.success) {
            notification.show(`Failed to launch: ${result.error}`, 'error');
        } else {
            // Reload to update last played
            loadLastPlayed();
        }
    };

    const getQuickLaunchLabel = () => {
        if (launching) return 'Launching...';
        if (!lastPlayedVersion) return 'No Versions';
        return lastLaunchMode === 'eac' ? 'Launch (EAC)' : 'Launch (No EAC)';
    };

    const getAlternateLaunchLabel = () => {
        return lastLaunchMode === 'eac' ? 'Launch (No EAC)' : 'Launch (EAC)';
    };

    const isArchiving = lastPlayedVersion && activeOperations.some(op => op.type === 'archive' && op.id === lastPlayedVersion.id);

    return (
        <footer className="sticky-footer">
            <div className="footer-left">
                <span className="footer-label">Last Played:</span>
                <span className="footer-value">
                    {isArchiving ? 'Archiving...' : (lastPlayedVersion ? lastPlayedVersion.name || `Version ${lastPlayedVersion.id}` : 'No versions yet')}
                </span>
            </div>

            <div className="footer-center">
                <button
                    className="quick-launch-btn"
                    onClick={handleQuickLaunch}
                    disabled={!lastPlayedVersion || launching || isArchiving}
                >
                    {isArchiving ? 'Archiving...' : getQuickLaunchLabel()}
                </button>
            </div>

            <div className="footer-right" ref={menuRef}>
                <button className="hamburger-btn" onClick={() => setShowMenu(!showMenu)} disabled={isArchiving}>
                    ☰
                </button>

                {showMenu && (
                    <div className="hamburger-menu">
                        <button
                            onClick={handleAlternateLaunch}
                            disabled={!lastPlayedVersion || launching}
                        >
                            {getAlternateLaunchLabel()}
                        </button>
                        <button onClick={() => { setShowMenu(false); onNavigate('library'); }}>
                            📚 Switch Version
                        </button>
                        <div className="menu-divider"></div>
                        <button onClick={() => { setShowMenu(false); onOpenSettings(); }}>
                            ⚙️ Settings
                        </button>
                        <button onClick={() => { setShowMenu(false); onOpenDownload(); }}>
                            ⬇️ Download Version
                        </button>
                    </div>
                )}
            </div>
        </footer>
    );
}
