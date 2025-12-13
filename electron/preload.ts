const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
    getAppVersion: () => ipcRenderer.invoke('get-app-version'),
    getVersions: () => ipcRenderer.invoke('get-versions'),
    getLastPlayedVersion: () => ipcRenderer.invoke('get-last-played-version'),
    getRustNews: () => ipcRenderer.invoke('get-rust-news'),
    startDownload: (config: any) => ipcRenderer.invoke('start-download', config),
    cancelDownload: () => ipcRenderer.invoke('cancel-download'),
    onDownloadProgress: (callback: (progress: any) => void) => {
        const listener = (_event: any, progress: any) => callback(progress);
        ipcRenderer.on('download-progress', listener);
        return () => ipcRenderer.removeListener('download-progress', listener);
    },
    onDownloadComplete: (callback: (result: any) => void) => {
        const listener = (_event: any, result: any) => callback(result);
        ipcRenderer.on('download-complete', listener);
        return () => ipcRenderer.removeListener('download-complete', listener);
    },
    launchVersion: (versionId: number, eacEnabled: boolean) => ipcRenderer.invoke('launch-version', versionId, eacEnabled),
    renameVersion: (versionId: number, newName: string) => ipcRenderer.invoke('rename-version', versionId, newName),
    deleteVersion: (versionId: number) => ipcRenderer.invoke('delete-version', versionId),
    archiveVersion: (versionId: number) => ipcRenderer.invoke('archive-version', versionId),
    browseFiles: (installPath: string) => ipcRenderer.invoke('browse-files', installPath),
    checkEacClient: (installPath: string) => ipcRenderer.invoke('check-eac-client', installPath),
    getSettings: () => ipcRenderer.invoke('get-settings'),
    setSetting: (key: string, value: string) => ipcRenderer.invoke('set-setting', key, value),
    deleteSetting: (key: string) => ipcRenderer.invoke('delete-setting', key),
    listArchives: () => ipcRenderer.invoke('list-archives'),
    unarchiveVersion: (archivePath: string, archiveName: string) => ipcRenderer.invoke('unarchive-version', archivePath, archiveName),
    purgeAllVersions: () => ipcRenderer.invoke('purge-all-versions'),
    archiveAllVersions: () => ipcRenderer.invoke('archive-all-versions'),
    getDiskUsage: () => ipcRenderer.invoke('get-disk-usage'),
    getCuratedVersionsRL: () => ipcRenderer.invoke('get-curated-versions-rl'),
    getCuratedVersionsOther: () => ipcRenderer.invoke('get-curated-versions-other'),
    // Install Paths
    getInstallPaths: () => ipcRenderer.invoke('get-install-paths'),
    addInstallPath: (installPath: string) => ipcRenderer.invoke('add-install-path', installPath),
    removeInstallPath: (id: number) => ipcRenderer.invoke('remove-install-path', id),
    getMostRecentPath: () => ipcRenderer.invoke('get-most-recent-path'),
    browseDirectory: () => ipcRenderer.invoke('browse-directory'),
    scanInstallPaths: () => ipcRenderer.invoke('scan-install-paths'),
    onArchiveProgress: (callback: (data: any) => void) => {
        const listener = (_event: any, data: any) => callback(data);
        ipcRenderer.on('archive-progress', listener);
        return () => ipcRenderer.removeListener('archive-progress', listener);
    },
    onUnarchiveProgress: (callback: (data: any) => void) => {
        const listener = (_event: any, data: any) => callback(data);
        ipcRenderer.on('unarchive-progress', listener);
        return () => ipcRenderer.removeListener('unarchive-progress', listener);
    },
});
