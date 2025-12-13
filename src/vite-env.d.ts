/// <reference types="vite/client" />

interface DownloadConfig {
    clientManifestId: string;
    bundlesManifestId: string;
    clientDepotId?: string;
    bundlesDepotId?: string;
    appId?: string;
    username?: string;
    password?: string;
    name?: string;  // Optional custom name
    installPath?: string;  // Optional custom install path
}

interface DownloadProgress {
    type: 'stdout' | 'stderr';
    data: string;
}

interface DownloadComplete {
    code: number | null;
}

interface LaunchResult {
    success: boolean;
    error?: string | null;
}

interface DeleteResult {
    success: boolean;
    error?: string;
}

interface RenameResult {
    success: boolean;
    error?: string;
}

interface ArchiveResult {
    success: boolean;
    error?: string;
}

interface BrowseResult {
    success: boolean;
    error?: string;
}

interface UnarchiveResult {
    success: boolean;
    error?: string;
}

interface SettingResult {
    success: boolean;
    error?: string;
}

interface PurgeAllResult {
    success: boolean;
    count?: number;
    error?: string;
}

interface ArchiveAllResult {
    success: boolean;
    count?: number;
    errors?: string[];
    error?: string;
}

interface AppSettings {
    steamUsername?: string;
    steamPassword?: string;
    lastLaunchMode?: string;
}

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

interface ArchivedVersion {
    filename: string;
    name: string;
    path: string;
    size: number;
}

interface CuratedVersion {
    title: string;
    clientManifest: string;
    bundleManifest: string;
    versionDate: string;
}

interface CuratedVersionsData {
    recommended: CuratedVersion[];
    other?: CuratedVersion[];
}

interface DiskUsage {
    libraryUsage: number;
    archiveUsage: number;
}

interface InstallPath {
    id: number;
    path: string;
    lastUsed: number;
}

interface AddInstallPathResult {
    success: boolean;
    error?: string;
    id?: number;
}

interface RemoveInstallPathResult {
    success: boolean;
}

interface Window {
    electronAPI: {
        getAppVersion: () => Promise<string>;
        getVersions: () => Promise<RustVersion[]>;
        getLastPlayedVersion: () => Promise<RustVersion | null>;
        getRustNews: () => Promise<any[]>;
        startDownload: (config: DownloadConfig) => Promise<void>;
        cancelDownload: () => Promise<void>;
        onDownloadProgress: (callback: (progress: DownloadProgress) => void) => () => void;
        onDownloadComplete: (callback: (result: DownloadComplete) => void) => () => void;
        launchVersion: (versionId: number, eacEnabled: boolean) => Promise<LaunchResult>;
        renameVersion: (versionId: number, newName: string) => Promise<RenameResult>;
        deleteVersion: (versionId: number) => Promise<DeleteResult>;
        archiveVersion: (versionId: number) => Promise<ArchiveResult>;
        browseFiles: (installPath: string) => Promise<BrowseResult>;
        checkEacClient: (installPath: string) => Promise<boolean>;
        getSettings: () => Promise<AppSettings>;
        setSetting: (key: string, value: string) => Promise<SettingResult>;
        deleteSetting: (key: string) => Promise<SettingResult>;
        listArchives: () => Promise<ArchivedVersion[]>;
        unarchiveVersion: (archivePath: string, archiveName: string) => Promise<UnarchiveResult>;
        purgeAllVersions: () => Promise<PurgeAllResult>;
        archiveAllVersions: () => Promise<ArchiveAllResult>;
        getDiskUsage: () => Promise<DiskUsage>;
        getCuratedVersionsRL: () => Promise<CuratedVersionsData>;
        getCuratedVersionsOther: () => Promise<CuratedVersionsData>;
        // Install Paths
        getInstallPaths: () => Promise<InstallPath[]>;
        addInstallPath: (installPath: string) => Promise<AddInstallPathResult>;
        removeInstallPath: (id: number) => Promise<RemoveInstallPathResult>;
        getMostRecentPath: () => Promise<string | null>;
        browseDirectory: () => Promise<string | null>;
        scanInstallPaths: () => Promise<ScanResult>;
        onArchiveProgress: (callback: (data: { id: number; progress: number; status: string }) => void) => () => void;
        onUnarchiveProgress: (callback: (data: { name: string; progress: number; log: string }) => void) => () => void;
    };
}

interface ScanResult {
    scanned: number;
    discovered: number;
    repaired: number;
}
