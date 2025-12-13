import { spawn, ChildProcess } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';
import fs from 'fs';

const require = createRequire(import.meta.url);
const electron = require('electron');
const { app } = electron;

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export interface DownloadConfig {
    clientManifestId: string;
    bundlesManifestId: string;
    clientDepotId?: string;
    bundlesDepotId?: string;
    appId?: string;
    username?: string;
    password?: string;
    installPath?: string;  // Custom install directory
}

export class DepotDownloaderService {
    private process: ChildProcess | null = null;
    private depotDownloaderPath: string;
    private currentClientDir: string | null = null;
    private currentTempBundlesDir: string | null = null;

    constructor() {
        // Path to the built DepotDownloader.dll
        this.depotDownloaderPath = path.join(
            __dirname,
            '../resources/DepotDownloader/DepotDownloader/bin/Release/net8.0/DepotDownloader.dll'
        );
    }

    startDualDownload(
        config: DownloadConfig,
        onData: (data: string) => void,
        onError: (error: string) => void,
        onComplete: (code: number | null) => void
    ): void {
        const appId = config.appId || '252490';
        const clientDepotId = config.clientDepotId || '252495';
        const bundlesDepotId = config.bundlesDepotId || '252494';

        // Normalize install path - ensure it ends with RustyLauncherVersions
        const normalizeInstallPath = (inputPath: string): string => {
            const normalized = path.normalize(inputPath);
            const basename = path.basename(normalized);

            // If path already ends with RustyLauncherVersions, use as-is
            if (basename === 'RustyLauncherVersions') {
                return normalized;
            }

            // Check if RustyLauncherVersions exists as a direct child
            const withRLV = path.join(normalized, 'RustyLauncherVersions');
            if (fs.existsSync(withRLV)) {
                return withRLV;
            }

            // Create RustyLauncherVersions subdirectory
            return withRLV;
        };

        // Use custom install path or default
        const rawPath = config.installPath || path.join(app.getPath('userData'), 'RustVersions');
        // Only normalize if using custom path, keep default as-is for backwards compatibility
        const basePath = config.installPath ? normalizeInstallPath(config.installPath) : rawPath;
        const clientDir = path.join(basePath, config.clientManifestId);
        const tempBundlesDir = path.join(basePath, 'tempBundles', config.bundlesManifestId);

        // Ensure directories exist
        if (!fs.existsSync(basePath)) {
            fs.mkdirSync(basePath, { recursive: true });
        }
        const archiveDir = path.join(basePath, 'archive');
        if (!fs.existsSync(archiveDir)) {
            fs.mkdirSync(archiveDir, { recursive: true });
        }
        const tempBundlesBase = path.join(basePath, 'tempBundles');
        if (!fs.existsSync(tempBundlesBase)) {
            fs.mkdirSync(tempBundlesBase, { recursive: true });
        }

        // Store paths for cancellation cleanup
        this.currentClientDir = clientDir;
        this.currentTempBundlesDir = tempBundlesDir;

        const cleanupAndComplete = (code: number | null) => {
            this.currentClientDir = null;
            this.currentTempBundlesDir = null;
            onComplete(code);
        };

        // Step 1: Download client depot
        onData('[Step 1/3] Downloading client files...\n');
        this.downloadDepot(
            appId,
            clientDepotId,
            config.clientManifestId,
            clientDir,
            config.username,
            config.password,
            onData,
            onError,
            (clientCode) => {
                if (clientCode !== 0) {
                    onError('[Step 1/3] Client download failed!\n');
                    cleanupAndComplete(clientCode);
                    return;
                }

                onData('[Step 1/3] Client download complete!\n');

                // Step 2: Download bundles depot
                onData('[Step 2/3] Downloading bundles...\n');
                this.downloadDepot(
                    appId,
                    bundlesDepotId,
                    config.bundlesManifestId,
                    tempBundlesDir,
                    config.username,
                    config.password,
                    onData,
                    onError,
                    (bundlesCode) => {
                        if (bundlesCode !== 0) {
                            onError('[Step 2/3] Bundles download failed!\n');
                            cleanupAndComplete(bundlesCode);
                            return;
                        }

                        onData('[Step 2/3] Bundles download complete!\n');

                        // Step 3: Move Bundles folder
                        onData('[Step 3/3] Merging bundles into version...\n');
                        this.mergeBundles(tempBundlesDir, clientDir, onData, onError, cleanupAndComplete);
                    }
                );
            }
        );
    }

    private downloadDepot(
        appId: string,
        depotId: string,
        manifestId: string,
        outputDir: string,
        username: string | undefined,
        password: string | undefined,
        onData: (data: string) => void,
        onError: (error: string) => void,
        onComplete: (code: number | null) => void
    ): void {
        const args = [
            this.depotDownloaderPath,
            '-app', appId,
            '-depot', depotId,
            '-manifest', manifestId,
            '-dir', outputDir,
        ];

        if (username) {
            args.push('-username', username);
        }
        if (password) {
            args.push('-password', password);
        }

        console.log('Starting DepotDownloader with args:', args);

        this.process = spawn('dotnet', args);

        this.process.stdout?.on('data', (data) => {
            onData(data.toString());
        });

        this.process.stderr?.on('data', (data) => {
            onError(data.toString());
        });

        this.process.on('close', (code) => {
            console.log(`DepotDownloader exited with code ${code}`);
            this.process = null;
            onComplete(code);
        });

        this.process.on('error', (err) => {
            onError(`Failed to start DepotDownloader: ${err.message}\n`);
            this.process = null;
            onComplete(null);
        });
    }

    private mergeBundles(
        tempBundlesDir: string,
        clientDir: string,
        onData: (data: string) => void,
        onError: (error: string) => void,
        onComplete: (code: number | null) => void
    ): void {
        try {
            const bundlesSource = path.join(tempBundlesDir, 'Bundles');
            const bundlesTarget = path.join(clientDir, 'Bundles');

            if (!fs.existsSync(bundlesSource)) {
                onError(`[Step 3/3] Bundles folder not found at: ${bundlesSource}\n`);
                onComplete(1);
                return;
            }

            // Move Bundles folder
            onData(`[Step 3/3] Moving Bundles from ${bundlesSource} to ${bundlesTarget}...\n`);
            fs.renameSync(bundlesSource, bundlesTarget);

            // Delete temp directory
            onData(`[Step 3/3] Cleaning up temporary files...\n`);
            fs.rmSync(tempBundlesDir, { recursive: true, force: true });

            onData('[Step 3/3] Merge complete! Version is ready to launch.\n');
            onComplete(0);
        } catch (error: any) {
            onError(`[Step 3/3] Failed to merge bundles: ${error.message}\n`);
            onComplete(1);
        }
    }

    cancel(): void {
        if (this.process) {
            this.process.kill();
            this.process = null;
        }

        // Cleanup files on cancel
        if (this.currentClientDir && fs.existsSync(this.currentClientDir)) {
            try {
                console.log('Cleaning up client dir after cancel:', this.currentClientDir);
                fs.rmSync(this.currentClientDir, { recursive: true, force: true });
            } catch (err) {
                console.error('Error cleaning up client dir:', err);
            }
        }

        if (this.currentTempBundlesDir && fs.existsSync(this.currentTempBundlesDir)) {
            try {
                console.log('Cleaning up temp bundles dir after cancel:', this.currentTempBundlesDir);
                fs.rmSync(this.currentTempBundlesDir, { recursive: true, force: true });
            } catch (err) {
                console.error('Error cleaning up temp bundles dir:', err);
            }
        }

        this.currentClientDir = null;
        this.currentTempBundlesDir = null;
    }
}
