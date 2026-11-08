import { ServerLibrary } from '../components/ServerLibrary';

interface ServersPageProps {
    refreshKey: number;
    onRefresh: () => void;
}

export function ServersPage({ refreshKey, onRefresh }: ServersPageProps) {
    return (
        <div className="servers-page">
            <ServerLibrary refreshKey={refreshKey} onRefresh={onRefresh} />
        </div>
    );
}
