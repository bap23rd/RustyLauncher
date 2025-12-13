import { createRequire } from 'module';
import path from 'path';
import { fileURLToPath } from 'url';
import Database from 'better-sqlite3';
import { DepotDownloaderService } from './downloader.js';
import { GameLauncher } from './launcher.js';
import https from 'https';
import fs from 'fs';
import Parser from 'rss-parser';
import { initSettingsDatabase, getSetting, setSetting, deleteSetting, getAllSettings, getInstallPaths, addInstallPath, removeInstallPath, updateLastUsedPath, getMostRecentPath } from './settings.js';
import archiver from 'archiver';
import extract from 'extract-zip';

const require = createRequire(import.meta.url);
const electron = require('electron');
const { app, BrowserWindow, ipcMain, shell, dialog } = electron;
const __dirname = path.dirname(fileURLToPath(import.meta.url));

let mainWindow: InstanceType<typeof BrowserWindow> | null = null;
let db: Database.Database | null = null;

function createWindow() {
    mainWindow = new BrowserWindow({
        width: 1200,
        height: 800,
        minWidth: 800,
        minHeight: 600,
        backgroundColor: '#281C1C',
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            nodeIntegration: false,
            contextIsolation: true,
        },
    });

    // Open external links in default browser
    mainWindow.webContents.setWindowOpenHandler(({ url }: { url: string }) => {
        const electron = require('electron');
        electron.shell.openExternal(url);
        return { action: 'deny' };
    });

    if (process.env.NODE_ENV === 'development') {
        mainWindow.loadURL('http://localhost:5173');
        mainWindow.webContents.openDevTools();
    } else {
        mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
    }
}

function initDatabase() {
    const dbPath = path.join(app.getPath('userData'), 'rusty_launcher.db');
    db = new Database(dbPath);
    db.pragma('journal_mode = WAL');

    // Create versions table with name and lastPlayed fields
    db.exec(`
    CREATE TABLE IF NOT EXISTS versions (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT,
      manifestId TEXT NOT NULL,
      depotId TEXT NOT NULL,
      buildDate TEXT,
      installPath TEXT NOT NULL,
      isEacEnabled INTEGER DEFAULT 1,
      lastPlayed INTEGER DEFAULT 0
    )
  `);
}

app.whenReady().then(() => {
    initDatabase();
    initSettingsDatabase();
    createWindow();

    app.on('activate', () => {
        if (BrowserWindow.getAllWindows().length === 0) {
            createWindow();
        }
    });
});

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        app.quit();
    }
});

// IPC Handlers
const downloaderService = new DepotDownloaderService();
const gameLauncher = new GameLauncher();

ipcMain.handle('get-app-version', () => {
    return app.getVersion();
});

ipcMain.handle('get-versions', () => {
    if (!db) return [];
    return db.prepare('SELECT * FROM versions ORDER BY lastPlayed DESC').all();
});

ipcMain.handle('get-last-played-version', () => {
    if (!db) return null;
    return db.prepare('SELECT * FROM versions ORDER BY lastPlayed DESC LIMIT 1').get();
});

ipcMain.handle('get-rust-news', async () => {
    try {
        const parser = new Parser();
        const feed = await parser.parseURL('https://rust.facepunch.com/rss/news');

        // Get top 5 items and format them
        const items = feed.items.slice(0, 5).map(item => {
            // Strip HTML tags from description
            const description = item.contentSnippet || item.content || '';
            const cleanDescription = description.replace(/<[^>]*>/g, '').trim();

            return {
                title: item.title || 'No title',
                contents: cleanDescription,
                date: new Date(item.pubDate || item.isoDate || '').getTime() / 1000,
                url: item.link || '',
                gid: item.guid || item.link || ''
            };
        });

        return items;
    } catch (error) {
        console.error('Error fetching Rust news from Facepunch RSS:', error);
        return [];
    }
});

ipcMain.handle('start-download', async (_event: any, config: any) => {
    // Generate name immediately to use for UI feedback
    let versionName = config.name;
    if (!versionName && db) {
        // Find highest existing "A Rust Version: X" number
        const existingVersions = db.prepare("SELECT name FROM versions WHERE name LIKE 'A Rust Version:%'").all() as any[];
        let maxNumber = 0;
        existingVersions.forEach((v: any) => {
            const match = v.name.match(/A Rust Version: (\d+)/);
            if (match) {
                maxNumber = Math.max(maxNumber, parseInt(match[1]));
            }
        });
        versionName = `A Rust Version: ${maxNumber + 1}`;
    }

    // Normalize install path - ensure custom paths end with RustyLauncherVersions
    const normalizeInstallPath = (inputPath: string): string => {
        const normalized = path.normalize(inputPath);
        const basename = path.basename(normalized);

        // If path already ends with RustyLauncherVersions, use as-is
        if (basename === 'RustyLauncherVersions') {
            return normalized;
        }

        // Append RustyLauncherVersions
        return path.join(normalized, 'RustyLauncherVersions');
    };

    // Determine install path - use custom (normalized) or default
    const rawPath = config.installPath || path.join(app.getPath('userData'), 'RustVersions');
    const basePath = config.installPath ? normalizeInstallPath(config.installPath) : rawPath;

    // Auto-add normalized path to install paths list if not already present
    if (config.installPath) {
        const normalizedPath = normalizeInstallPath(config.installPath);
        const existingPaths = getInstallPaths();
        const pathExists = existingPaths.some(p => path.normalize(p.path) === normalizedPath);
        if (!pathExists) {
            addInstallPath(normalizedPath);
        }
        // Update most recently used path
        updateLastUsedPath(normalizedPath);
    }

    downloaderService.startDualDownload(
        config,
        (data) => {
            mainWindow?.webContents.send('download-progress', { type: 'stdout', data, name: versionName });
        },
        (error) => {
            mainWindow?.webContents.send('download-progress', { type: 'stderr', data: error, name: versionName });
        },
        (code) => {
            mainWindow?.webContents.send('download-complete', { code, name: versionName });

            // Save to database if successful
            if (code === 0 && db) {
                const outputDir = path.join(basePath, config.clientManifestId);

                db.prepare(
                    'INSERT INTO versions (name, manifestId, depotId, buildDate, installPath, isEacEnabled, lastPlayed) VALUES (?, ?, ?, ?, ?, ?, ?)'
                ).run(versionName, config.clientManifestId, config.clientDepotId || '252495', new Date().toISOString(), outputDir, 1, 0);

                // Save metadata file to preserve name for future rescans
                try {
                    const metadataPath = path.join(outputDir, '.rustylauncher.json');
                    const metadata = {
                        name: versionName,
                        manifestId: config.clientManifestId,
                        downloadDate: new Date().toISOString()
                    };
                    fs.writeFileSync(metadataPath, JSON.stringify(metadata, null, 2));
                } catch (err) {
                    console.error('Error saving version metadata:', err);
                }
            } else if (code !== 0) {
                // Cleanup on failure
                try {
                    const versionDir = path.join(basePath, config.clientManifestId);
                    if (fs.existsSync(versionDir)) {
                        fs.rmSync(versionDir, { recursive: true, force: true });
                        console.log(`Cleaned up failed download version dir: ${versionDir}`);
                    }

                    if (config.bundlesManifestId) {
                        const tempBundlesDir = path.join(basePath, 'tempBundles', config.bundlesManifestId);
                        if (fs.existsSync(tempBundlesDir)) {
                            fs.rmSync(tempBundlesDir, { recursive: true, force: true });
                            console.log(`Cleaned up failed download bundles dir: ${tempBundlesDir}`);
                        }
                    }
                } catch (err) {
                    console.error('Error cleaning up failed download:', err);
                }
            }
        }
    );
});

ipcMain.handle('cancel-download', () => {
    downloaderService.cancel();
});

ipcMain.handle('launch-version', (_event: any, versionId: number, eacEnabled: boolean) => {
    if (!db) return { success: false, error: 'Database not initialized' };

    const version = db.prepare('SELECT * FROM versions WHERE id = ?').get(versionId) as any;
    if (!version) {
        return { success: false, error: 'Version not found' };
    }

    // Update lastPlayed timestamp
    db.prepare('UPDATE versions SET lastPlayed = ? WHERE id = ?').run(Date.now(), versionId);

    // Save last launch mode to settings
    setSetting('lastLaunchMode', eacEnabled ? 'eac' : 'no-eac');

    if (eacEnabled) {
        const success = gameLauncher.launchWithEAC(version.installPath);
        return { success, error: success ? null : 'Rust.exe not found or failed to launch' };
    } else {
        const success = gameLauncher.launchWithoutEAC(version.installPath);
        return { success, error: success ? null : 'RustClient.exe not found or failed to launch' };
    }
});

ipcMain.handle('rename-version', (_event: any, versionId: number, newName: string) => {
    if (!db) return { success: false, error: 'Database not initialized' };

    try {
        // Get the version to find its installPath
        const version = db.prepare('SELECT * FROM versions WHERE id = ?').get(versionId) as any;

        db.prepare('UPDATE versions SET name = ? WHERE id = ?').run(newName, versionId);

        // Also update the metadata file to persist the name
        if (version && version.installPath) {
            try {
                const metadataPath = path.join(version.installPath, '.rustylauncher.json');
                let metadata: any = { name: newName };

                // Read existing metadata and update
                if (fs.existsSync(metadataPath)) {
                    const existingContent = fs.readFileSync(metadataPath, 'utf-8');
                    metadata = { ...JSON.parse(existingContent), name: newName };
                } else {
                    metadata = {
                        name: newName,
                        manifestId: version.manifestId,
                        downloadDate: version.buildDate
                    };
                }

                fs.writeFileSync(metadataPath, JSON.stringify(metadata, null, 2));
            } catch (err) {
                console.error('Error updating version metadata file:', err);
            }
        }

        return { success: true };
    } catch (error: any) {
        return { success: false, error: error.message };
    }
});

ipcMain.handle('delete-version', (_event: any, versionId: number) => {
    if (!db) return { success: false, error: 'Database not initialized' };

    const version = db.prepare('SELECT * FROM versions WHERE id = ?').get(versionId) as any;
    if (!version) {
        return { success: false, error: 'Version not found' };
    }

    try {
        // Delete from disk
        if (fs.existsSync(version.installPath)) {
            fs.rmSync(version.installPath, { recursive: true, force: true });
        }

        // Delete from database
        db.prepare('DELETE FROM versions WHERE id = ?').run(versionId);

        return { success: true };
    } catch (error: any) {
        return { success: false, error: error.message };
    }
});

ipcMain.handle('archive-version', async (_event: any, versionId: number) => {
    console.log('Archive handler called for version ID:', versionId);
    if (!db) return { success: false, error: 'Database not initialized' };

    const version = db.prepare('SELECT * FROM versions WHERE id = ?').get(versionId) as any;
    if (!version) {
        console.error('Version not found:', versionId);
        return { success: false, error: 'Version not found' };
    }

    console.log('Archiving version:', version);

    // Check if install path exists
    if (!fs.existsSync(version.installPath)) {
        console.error('Install path does not exist:', version.installPath);
        return { success: false, error: 'Version files not found on disk' };
    }

    try {
        // Derive archive directory from the version's install path
        // The version's installPath is like: /path/to/RustyLauncherVersions/manifestId
        // We want the archive at: /path/to/RustyLauncherVersions/archive
        const versionParentDir = path.dirname(version.installPath);
        const archiveDir = path.join(versionParentDir, 'archive');

        if (!fs.existsSync(archiveDir)) {
            console.log('Creating archive directory:', archiveDir);
            fs.mkdirSync(archiveDir, { recursive: true });
        }

        // Calculate total size for progress
        const totalSize = getDirSize(version.installPath);
        console.log('Total version size:', totalSize);

        // Sanitize filename - remove invalid Windows characters: < > : " / \ | ? *
        const sanitizedName = (version.name || version.manifestId).replace(/[<>:"/\\|?*]/g, '_');
        const archivePath = path.join(archiveDir, `${sanitizedName}.zip`);
        console.log('Archive path:', archivePath);

        return new Promise((resolve) => {
            const output = fs.createWriteStream(archivePath);
            const archive = archiver('zip', {
                zlib: { level: 5 } // Reduced compression for faster archiving
            });

            output.on('close', () => {
                console.log('Archive created successfully, size:', archive.pointer(), 'bytes');
                console.log('Archive file path:', archivePath);

                // Send final completion message
                mainWindow?.webContents.send('archive-progress', { id: versionId, progress: 100, status: 'Completed' });

                // Verify archive file exists
                if (fs.existsSync(archivePath)) {
                    const stats = fs.statSync(archivePath);
                    console.log('Archive file confirmed on disk, size:', stats.size, 'bytes');
                } else {
                    console.error('WARNING: Archive file not found at:', archivePath);
                }

                // Delete from disk after archiving
                if (fs.existsSync(version.installPath)) {
                    console.log('Deleting original files:', version.installPath);
                    fs.rmSync(version.installPath, { recursive: true, force: true });
                }

                // Delete from database
                db!.prepare('DELETE FROM versions WHERE id = ?').run(versionId);
                console.log('Version removed from database');

                resolve({ success: true });
            });

            archive.on('error', (err: any) => {
                console.error('Archive error:', err);
                resolve({ success: false, error: err.message });
            });

            archive.on('warning', (err: any) => {
                console.warn('Archive warning:', err);
            });

            archive.on('progress', (progress) => {
                const percent = totalSize > 0 ? Math.round((progress.fs.processedBytes / totalSize) * 100) : 0;
                mainWindow?.webContents.send('archive-progress', {
                    id: versionId,
                    progress: percent,
                    status: 'Archiving...'
                });
            });

            console.log('Starting archive process...');
            archive.pipe(output);
            archive.directory(version.installPath, false);
            archive.finalize();
        });
    } catch (error: any) {
        console.error('Archive exception:', error);
        return { success: false, error: error.message };
    }
});

ipcMain.handle('browse-files', (_event: any, installPath: string) => {
    try {
        shell.openPath(installPath);
        return { success: true };
    } catch (error: any) {
        return { success: false, error: error.message };
    }
});

ipcMain.handle('check-eac-client', (_event: any, installPath: string) => {
    return gameLauncher.checkEACClientExists(installPath);
});

// Settings IPC Handlers
ipcMain.handle('get-settings', () => {
    return getAllSettings();
});

ipcMain.handle('set-setting', (_event: any, key: string, value: string) => {
    setSetting(key, value);
    return { success: true };
});

ipcMain.handle('delete-setting', (_event: any, key: string) => {
    deleteSetting(key);
    return { success: true };
});

// Curated Versions IPC Handlers
ipcMain.handle('get-curated-versions-rl', () => {
    try {
        const filePath = path.join(app.getPath('userData'), 'versionRL.json');
        if (!fs.existsSync(filePath)) {
            return { recommended: [] };
        }
        const data = fs.readFileSync(filePath, 'utf-8');
        return JSON.parse(data);
    } catch (error) {
        console.error('Error reading versionRL.json:', error);
        return { recommended: [] };
    }
});

ipcMain.handle('get-curated-versions-other', () => {
    try {
        const filePath = path.join(app.getPath('userData'), 'versionOther.json');
        if (!fs.existsSync(filePath)) {
            return { recommended: [] };
        }
        const data = fs.readFileSync(filePath, 'utf-8');
        return JSON.parse(data);
    } catch (error) {
        console.error('Error reading versionOther.json:', error);
        return { recommended: [] };
    }
});


// Install Paths handlers
ipcMain.handle('get-install-paths', () => {
    return getInstallPaths();
});

ipcMain.handle('add-install-path', (_event: any, installPath: string) => {
    return addInstallPath(installPath);
});

ipcMain.handle('remove-install-path', (_event: any, id: number) => {
    return removeInstallPath(id);
});

ipcMain.handle('get-most-recent-path', () => {
    return getMostRecentPath();
});

ipcMain.handle('browse-directory', async () => {
    if (!mainWindow) return null;

    const result = await dialog.showOpenDialog(mainWindow, {
        properties: ['openDirectory'],
        title: 'Select Install Directory'
    });

    if (result.canceled || result.filePaths.length === 0) {
        return null;
    }

    return result.filePaths[0];
});

// Scan install paths for versions and repair folder structure
ipcMain.handle('scan-install-paths', () => {
    if (!db) return { scanned: 0, discovered: 0, repaired: 0 };

    const installPaths = getInstallPaths();
    let scannedPaths = 0;
    let discoveredVersions = 0;
    let repairedDirs = 0;

    // Get existing versions from DB to compare
    const existingVersions = db.prepare('SELECT installPath FROM versions').all() as { installPath: string }[];
    const existingPaths = new Set(existingVersions.map(v => path.normalize(v.installPath)));

    for (const installPath of installPaths) {
        const basePath = installPath.path;

        // Normalize the path - ensure it ends with RustyLauncherVersions
        let scanPath = basePath;
        const basename = path.basename(basePath);
        if (basename !== 'RustyLauncherVersions') {
            scanPath = path.join(basePath, 'RustyLauncherVersions');
        }

        // Skip if base path doesn't exist
        if (!fs.existsSync(scanPath)) {
            continue;
        }

        scannedPaths++;

        // Repair: ensure archive and tempBundles exist
        const archiveDir = path.join(scanPath, 'archive');
        const tempBundlesDir = path.join(scanPath, 'tempBundles');

        if (!fs.existsSync(archiveDir)) {
            fs.mkdirSync(archiveDir, { recursive: true });
            repairedDirs++;
        }

        if (!fs.existsSync(tempBundlesDir)) {
            fs.mkdirSync(tempBundlesDir, { recursive: true });
            repairedDirs++;
        }

        // Scan for version folders (directories that aren't archive or tempBundles)
        const entries = fs.readdirSync(scanPath, { withFileTypes: true });
        for (const entry of entries) {
            if (!entry.isDirectory()) continue;
            if (entry.name === 'archive' || entry.name === 'tempBundles') continue;

            const versionPath = path.join(scanPath, entry.name);
            const normalizedVersionPath = path.normalize(versionPath);

            // Check if this version is already in the database
            if (existingPaths.has(normalizedVersionPath)) {
                continue;
            }

            // Check if it looks like a valid Rust version (has RustClient.exe)
            const rustClientPath = path.join(versionPath, 'RustClient.exe');
            if (!fs.existsSync(rustClientPath)) {
                continue; // Not a valid Rust install
            }

            // Try to read metadata file for saved name
            const manifestId = entry.name; // The folder name is typically the manifest ID
            let versionName = `Discovered: ${entry.name.substring(0, 16)}...`;
            let downloadDate = new Date().toISOString();

            const metadataPath = path.join(versionPath, '.rustylauncher.json');
            if (fs.existsSync(metadataPath)) {
                try {
                    const metadataContent = fs.readFileSync(metadataPath, 'utf-8');
                    const metadata = JSON.parse(metadataContent);
                    if (metadata.name) {
                        versionName = metadata.name;
                    }
                    if (metadata.downloadDate) {
                        downloadDate = metadata.downloadDate;
                    }
                } catch (err) {
                    console.error('Error reading version metadata:', err);
                }
            }

            try {
                db.prepare(
                    'INSERT INTO versions (name, manifestId, depotId, buildDate, installPath, isEacEnabled, lastPlayed) VALUES (?, ?, ?, ?, ?, ?, ?)'
                ).run(versionName, manifestId, '252495', downloadDate, versionPath, 1, 0);
                discoveredVersions++;
                existingPaths.add(normalizedVersionPath); // Prevent duplicates in this scan
            } catch (err) {
                console.error('Error adding discovered version:', err);
            }
        }
    }

    return { scanned: scannedPaths, discovered: discoveredVersions, repaired: repairedDirs };
});

ipcMain.handle('list-archives', () => {
    try {
        const installPaths = getInstallPaths();
        const allArchives: any[] = [];

        for (const installPath of installPaths) {
            let basePath = installPath.path;
            const basename = path.basename(basePath);

            // Normalize to RustyLauncherVersions
            if (basename !== 'RustyLauncherVersions') {
                basePath = path.join(basePath, 'RustyLauncherVersions');
            }

            const archiveDir = path.join(basePath, 'archive');
            if (!fs.existsSync(archiveDir)) {
                continue;
            }

            const files = fs.readdirSync(archiveDir);
            const archives = files
                .filter(file => file.endsWith('.zip'))
                .map(file => ({
                    filename: file,
                    name: file.replace('.zip', ''),
                    path: path.join(archiveDir, file),
                    size: fs.statSync(path.join(archiveDir, file)).size,
                }));

            allArchives.push(...archives);
        }

        return allArchives;
    } catch (error: any) {
        console.error('Error listing archives:', error);
        return [];
    }
});

ipcMain.handle('unarchive-version', async (_event: any, archivePath: string, archiveName: string) => {
    if (!db) return { success: false, error: 'Database not initialized' };

    try {
        // Derive extract path from archive location
        // Archive is at: /path/to/RustyLauncherVersions/archive/name.zip
        // We want to extract to: /path/to/RustyLauncherVersions/unarchived_timestamp
        const archiveParentDir = path.dirname(archivePath); // .../archive
        const versionsDir = path.dirname(archiveParentDir); // .../RustyLauncherVersions

        const timestamp = Date.now().toString();
        const extractPath = path.join(versionsDir, `unarchived_${timestamp}`);

        let processed = 0;

        // Extract the zip
        await extract(archivePath, {
            dir: extractPath,
            onEntry: (entry, zipfile) => {
                processed++;
                const percent = Math.round((processed / zipfile.entryCount) * 100);
                mainWindow?.webContents.send('unarchive-progress', {
                    name: archiveName,
                    progress: percent,
                    log: `Extracting ${entry.fileName}`
                });
            }
        });

        // Add to database
        db.prepare(
            'INSERT INTO versions (name, manifestId, depotId, buildDate, installPath, isEacEnabled, lastPlayed) VALUES (?, ?, ?, ?, ?, ?, ?)'
        ).run(archiveName, `unarchived_${timestamp}`, 'unknown', new Date().toISOString(), extractPath, 1, 0);

        // Delete the archive file after success
        if (fs.existsSync(archivePath)) {
            fs.rmSync(archivePath, { force: true });
            console.log('Archive deleted:', archivePath);
        }

        return { success: true };
    } catch (error: any) {
        return { success: false, error: error.message };
    }
});

ipcMain.handle('purge-all-versions', async () => {
    if (!db) return { success: false, error: 'Database not initialized' };

    try {
        const versions = db.prepare('SELECT * FROM versions').all() as any[];

        for (const version of versions) {
            if (fs.existsSync(version.installPath)) {
                fs.rmSync(version.installPath, { recursive: true, force: true });
            }
        }

        db.prepare('DELETE FROM versions').run();

        return { success: true, count: versions.length };
    } catch (error: any) {
        return { success: false, error: error.message };
    }
});

ipcMain.handle('archive-all-versions', async () => {
    if (!db) return { success: false, error: 'Database not initialized' };

    try {
        const versions = db.prepare('SELECT * FROM versions').all() as any[];
        const archiveDir = path.join(app.getPath('userData'), 'RustVersions', 'archive');

        if (!fs.existsSync(archiveDir)) {
            fs.mkdirSync(archiveDir, { recursive: true });
        }

        let successCount = 0;
        const errors: string[] = [];

        for (const version of versions) {
            try {
                const sanitizedName = (version.name || version.manifestId).replace(/[<>:"/\\|?*]/g, '_');
                const archivePath = path.join(archiveDir, `${sanitizedName}.zip`);

                await new Promise<void>((resolve, reject) => {
                    const output = fs.createWriteStream(archivePath);
                    const archive = archiver('zip', { zlib: { level: 5 } });

                    output.on('close', () => {
                        if (fs.existsSync(version.installPath)) {
                            fs.rmSync(version.installPath, { recursive: true, force: true });
                        }
                        resolve();
                    });

                    archive.on('error', (err: any) => {
                        reject(err);
                    });

                    archive.pipe(output);
                    archive.directory(version.installPath, false);
                    archive.finalize();
                });

                successCount++;
            } catch (error: any) {
                errors.push(`${version.name || version.manifestId}: ${error.message}`);
            }
        }

        // Remove all from database
        db.prepare('DELETE FROM versions').run();

        return {
            success: true,
            count: successCount,
            errors: errors.length > 0 ? errors : undefined
        };
    } catch (error: any) {
        return { success: false, error: error.message };
    }
});

function getDirSize(dirPath: string): number {
    let size = 0;
    try {
        if (!fs.existsSync(dirPath)) return 0;
        const files = fs.readdirSync(dirPath);
        for (const file of files) {
            const filePath = path.join(dirPath, file);
            const stats = fs.statSync(filePath);
            if (stats.isDirectory()) {
                size += getDirSize(filePath);
            } else {
                size += stats.size;
            }
        }
    } catch (error) {
        console.error('Error calculating dir size:', error);
    }
    return size;
}

ipcMain.handle('get-disk-usage', async () => {
    try {
        const installPaths = getInstallPaths();
        let libraryUsage = 0;
        let archiveUsage = 0;

        for (const installPath of installPaths) {
            let basePath = installPath.path;
            const basename = path.basename(basePath);

            // Normalize to RustyLauncherVersions
            if (basename !== 'RustyLauncherVersions') {
                basePath = path.join(basePath, 'RustyLauncherVersions');
            }

            if (!fs.existsSync(basePath)) {
                continue;
            }

            // Calculate archive usage for this path
            const archiveDir = path.join(basePath, 'archive');
            if (fs.existsSync(archiveDir)) {
                archiveUsage += getDirSize(archiveDir);
            }

            // Calculate library usage (all folders except archive and tempBundles)
            const files = fs.readdirSync(basePath);
            for (const file of files) {
                const filePath = path.join(basePath, file);
                if (file !== 'archive' && file !== 'tempBundles') {
                    const stats = fs.statSync(filePath);
                    if (stats.isDirectory()) {
                        libraryUsage += getDirSize(filePath);
                    } else {
                        libraryUsage += stats.size;
                    }
                }
            }
        }

        return { libraryUsage, archiveUsage };
    } catch (error: any) {
        console.error('Error getting disk usage:', error);
        return { libraryUsage: 0, archiveUsage: 0 };
    }
});
