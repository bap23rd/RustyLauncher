import { VersionLibrary } from '../components/VersionLibrary';

interface LibraryPageProps {
    onRefresh: () => void;
    refreshKey: number;
    activeOperations: any[]; // Using any[] to avoid import loop, or define interface locally
    onOpenInstallPaths?: () => void;
}

export function LibraryPage({ onRefresh, refreshKey, activeOperations, onOpenInstallPaths }: LibraryPageProps) {
    return (
        <div className="library-page">
            <VersionLibrary key={refreshKey} onRefresh={onRefresh} activeOperations={activeOperations} onOpenInstallPaths={onOpenInstallPaths} />
        </div>
    );
}
