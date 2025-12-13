export interface CuratedVersion {
    title: string;
    clientManifest: string;
    bundleManifest: string;
    versionDate: string;
}

export interface CuratedVersionsData {
    recommended?: CuratedVersion[];
    other?: CuratedVersion[];
}
