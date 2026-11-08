import { useEffect, useState } from 'react';
import { PlayCircle, BookOpen, DownloadSimple, HardDrives } from '@phosphor-icons/react';
import '../App.css';

interface HomePageProps {
    onNavigateLibrary: () => void;
    onNavigateDownloads: () => void;
    onNavigateServers: () => void;
}

export function HomePage({ onNavigateLibrary, onNavigateDownloads, onNavigateServers }: HomePageProps) {
    const [lastPlayedVersion, setLastPlayedVersion] = useState<RustVersion | null>(null);
    const [versionCount, setVersionCount] = useState(0);
    const [serverCount, setServerCount] = useState(0);

    useEffect(() => {
        window.electronAPI.getLastPlayedVersion().then(setLastPlayedVersion);
        window.electronAPI.getVersions().then(versions => setVersionCount(versions.length));
        window.electronAPI.getServers().then(servers => setServerCount(servers.length));
    }, []);

    return (
        <section className="dashboard">
            <div className="dashboard-grid">
                <div className="dashboard-card">
                    <h3>Last Played</h3>
                    <p className="dashboard-card-value">
                        {lastPlayedVersion ? (lastPlayedVersion.name || `Version ${lastPlayedVersion.id}`) : 'No versions yet'}
                    </p>
                    <button className="dashboard-card-action" onClick={onNavigateLibrary}>
                        <PlayCircle size={18} weight="bold" /> Go to Library
                    </button>
                </div>

                <div className="dashboard-card">
                    <h3>Game Versions</h3>
                    <p className="dashboard-card-value">{versionCount} installed</p>
                    <button className="dashboard-card-action" onClick={onNavigateLibrary}>
                        <BookOpen size={18} weight="bold" /> View Library
                    </button>
                </div>

                <div className="dashboard-card">
                    <h3>Get a New Version</h3>
                    <p className="dashboard-card-value">Download a curated or custom build</p>
                    <button className="dashboard-card-action" onClick={onNavigateDownloads}>
                        <DownloadSimple size={18} weight="bold" /> Download Version
                    </button>
                </div>

                <div className="dashboard-card">
                    <h3>Servers</h3>
                    <p className="dashboard-card-value">{serverCount} installed</p>
                    <button className="dashboard-card-action" onClick={onNavigateServers}>
                        <HardDrives size={18} weight="bold" /> View Servers
                    </button>
                </div>
            </div>
        </section>
    );
}
