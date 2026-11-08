import { spawn, ChildProcess } from 'child_process';
import path from 'path';
import fs from 'fs';
import https from 'https';
import extractZip from 'extract-zip';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);
const electron = require('electron');
const { app } = electron;

const STEAMCMD_ZIP_URL = 'https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip';

/**
 * Wraps SteamCMD for "always latest" installs (used for Rust Dedicated Server,
 * which is free/anonymous-login, unlike the depot-pinned game client downloads).
 * SteamCMD isn't bundled with the app - it's fetched into userData on first use.
 */
export class SteamCmdService {
    private process: ChildProcess | null = null;

    private get steamCmdDir(): string {
        return path.join(app.getPath('userData'), 'SteamCMD');
    }

    private get steamCmdExePath(): string {
        return path.join(this.steamCmdDir, 'steamcmd.exe');
    }

    async ensureInstalled(onData: (data: string) => void): Promise<string> {
        if (fs.existsSync(this.steamCmdExePath)) {
            return this.steamCmdExePath;
        }

        onData('[SteamCMD] Not found locally - downloading...\n');
        fs.mkdirSync(this.steamCmdDir, { recursive: true });

        const zipPath = path.join(this.steamCmdDir, 'steamcmd.zip');
        await this.downloadFile(STEAMCMD_ZIP_URL, zipPath);

        onData('[SteamCMD] Extracting...\n');
        await extractZip(zipPath, { dir: this.steamCmdDir });
        fs.rmSync(zipPath, { force: true });

        if (!fs.existsSync(this.steamCmdExePath)) {
            throw new Error('SteamCMD download succeeded but steamcmd.exe was not found after extraction.');
        }

        onData('[SteamCMD] Ready.\n');
        return this.steamCmdExePath;
    }

    private downloadFile(url: string, destPath: string): Promise<void> {
        return new Promise((resolve, reject) => {
            const file = fs.createWriteStream(destPath);
            https.get(url, (response) => {
                if (response.statusCode && response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
                    file.close();
                    this.downloadFile(response.headers.location, destPath).then(resolve, reject);
                    return;
                }
                if (response.statusCode !== 200) {
                    file.close();
                    reject(new Error(`Failed to download SteamCMD: HTTP ${response.statusCode}`));
                    return;
                }
                response.pipe(file);
                file.on('finish', () => file.close(() => resolve()));
            }).on('error', (err) => {
                fs.rmSync(destPath, { force: true });
                reject(err);
            });
        });
    }

    /**
     * Installs/updates an app to its latest public branch via anonymous login.
     * Suitable for Rust Dedicated Server (appId 258550), which doesn't require a Steam account.
     */
    async installLatest(
        appId: string,
        installDir: string,
        onData: (data: string) => void,
        onError: (error: string) => void,
        onComplete: (code: number | null) => void
    ): Promise<void> {
        let steamCmdExe: string;
        try {
            steamCmdExe = await this.ensureInstalled(onData);
        } catch (err: any) {
            onError(`[SteamCMD] Setup failed: ${err.message}\n`);
            onComplete(1);
            return;
        }

        fs.mkdirSync(installDir, { recursive: true });

        const args = [
            '+force_install_dir', installDir,
            '+login', 'anonymous',
            '+app_update', appId, 'validate',
            '+quit',
        ];

        onData(`[SteamCMD] Installing app ${appId} (latest) to ${installDir}...\n`);
        this.process = spawn(steamCmdExe, args);

        this.process.stdout?.on('data', (data) => onData(data.toString()));
        this.process.stderr?.on('data', (data) => onError(data.toString()));

        this.process.on('close', (code) => {
            this.process = null;
            onComplete(code);
        });

        this.process.on('error', (err) => {
            onError(`Failed to start SteamCMD: ${err.message}\n`);
            this.process = null;
            onComplete(null);
        });
    }

    cancel(): void {
        if (this.process) {
            this.process.kill();
            this.process = null;
        }
    }
}
