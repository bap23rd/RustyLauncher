import { House, BookOpen, DownloadSimple, GearSix, HardDrives } from '@phosphor-icons/react';
import './Sidebar.css';

type Page = 'home' | 'library' | 'downloads' | 'servers';

interface SidebarProps {
    currentPage: Page;
    onNavigate: (page: Page) => void;
    onOpenSettings: () => void;
}

export function Sidebar({ currentPage, onNavigate, onOpenSettings }: SidebarProps) {
    return (
        <nav className="sidebar">
            <div className="sidebar-brand">
                <span className="sidebar-brand-name">Rusty<span className="sidebar-brand-accent">Launcher</span></span>
            </div>

            <div className="sidebar-section">
                <span className="sidebar-section-label">Game</span>
                <button
                    className={`sidebar-link ${currentPage === 'home' ? 'active' : ''}`}
                    onClick={() => onNavigate('home')}
                >
                    <House size={20} weight="bold" /> Home
                </button>
                <button
                    className={`sidebar-link ${currentPage === 'library' ? 'active' : ''}`}
                    onClick={() => onNavigate('library')}
                >
                    <BookOpen size={20} weight="bold" /> Library
                </button>
                <button
                    className={`sidebar-link ${currentPage === 'downloads' ? 'active' : ''}`}
                    onClick={() => onNavigate('downloads')}
                >
                    <DownloadSimple size={20} weight="bold" /> Download
                </button>
            </div>

            <div className="sidebar-section">
                <span className="sidebar-section-label">Servers</span>
                <button
                    className={`sidebar-link ${currentPage === 'servers' ? 'active' : ''}`}
                    onClick={() => onNavigate('servers')}
                >
                    <HardDrives size={20} weight="bold" /> Servers
                </button>
            </div>

            <div className="sidebar-footer">
                <button className="sidebar-link" onClick={onOpenSettings}>
                    <GearSix size={20} weight="bold" /> Settings
                </button>
            </div>
        </nav>
    );
}
