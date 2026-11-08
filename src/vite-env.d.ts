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
        // Servers
        getServers: () => Promise<RustServer[]>;
        startServerDownloadManifest: (config: ServerDownloadManifestConfig) => Promise<void>;
        startServerDownloadLatest: (config: ServerDownloadLatestConfig) => Promise<void>;
        cancelServerDownload: () => Promise<void>;
        renameServer: (serverId: number, newName: string) => Promise<ServerActionResult>;
        deleteServer: (serverId: number) => Promise<ServerActionResult>;
        browseServerFiles: (installPath: string) => Promise<ServerActionResult>;
        getServerConfig: (serverId: number) => Promise<ServerConfigResponse | null>;
        saveServerConfig: (serverId: number, config: ServerLaunchConfig) => Promise<ServerActionResult>;
        openServerModsFolder: (serverId: number) => Promise<ServerActionResult>;
        onServerDownloadProgress: (callback: (progress: DownloadProgress & { name: string }) => void) => () => void;
        onServerDownloadComplete: (callback: (result: DownloadComplete & { name: string }) => void) => () => void;
    };
}

interface ScanResult {
    scanned: number;
    discovered: number;
    repaired: number;
}

interface RustServer {
    id: number;
    name: string | null;
    installType: 'manifest' | 'latest';
    appId: string;
    depotId: string | null;
    manifestId: string | null;
    installPath: string;
    createdAt: string;
}

interface ServerDownloadManifestConfig {
    name?: string;
    appId?: string;
    depotId: string;
    manifestId: string;
    installPath?: string;
    username?: string;
    password?: string;
}

interface ServerDownloadLatestConfig {
    name?: string;
    appId?: string;
    installPath?: string;
}

interface ServerActionResult {
    success: boolean;
    error?: string;
}

interface ServerLaunchArg {
    enabled: boolean;
    value?: string;
}

interface ServerLaunchConfig {
    hostname: ServerLaunchArg;
    identity: ServerLaunchArg;
    port: ServerLaunchArg;
    rconPort: ServerLaunchArg;
    rconPassword: ServerLaunchArg;
    rconWeb: { enabled: boolean };
    maxPlayers: ServerLaunchArg;
    worldSize: ServerLaunchArg;
    seed: ServerLaunchArg;
    saveInterval: ServerLaunchArg;
    customArgs: string;
}

interface ServerConfigResponse {
    config: ServerLaunchConfig;
    hasOxide: boolean;
}
