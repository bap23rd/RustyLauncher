import Database from 'better-sqlite3';
import path from 'path';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);
const electron = require('electron');
const { app } = electron;

let settingsDb: Database.Database | null = null;

export interface AppSettings {
    steamUsername?: string;
    steamPassword?: string;
}

export interface InstallPath {
    id: number;
    path: string;
    lastUsed: number;
}

export function initSettingsDatabase(): void {
    const dbPath = path.join(app.getPath('userData'), 'settings.db');
    settingsDb = new Database(dbPath);
    settingsDb.pragma('journal_mode = WAL');

    // Create settings table
    settingsDb.exec(`
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
    `);

    // Create install_paths table
    settingsDb.exec(`
        CREATE TABLE IF NOT EXISTS install_paths (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT UNIQUE NOT NULL,
            lastUsed INTEGER DEFAULT 0
        )
    `);

    // Add default AppData path if no paths exist
    const existingPaths = getInstallPaths();
    if (existingPaths.length === 0) {
        const defaultPath = path.join(app.getPath('userData'), 'RustVersions');
        addInstallPath(defaultPath);
    }
}

export function getSetting(key: string): string | null {
    if (!settingsDb) return null;

    const row = settingsDb.prepare('SELECT value FROM settings WHERE key = ?').get(key) as any;
    return row ? row.value : null;
}

export function setSetting(key: string, value: string): void {
    if (!settingsDb) return;

    settingsDb.prepare('INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)').run(key, value);
}

export function deleteSetting(key: string): void {
    if (!settingsDb) return;

    settingsDb.prepare('DELETE FROM settings WHERE key = ?').run(key);
}

export function getAllSettings(): AppSettings {
    if (!settingsDb) return {};

    const rows = settingsDb.prepare('SELECT key, value FROM settings').all() as any[];
    const settings: any = {};

    rows.forEach(row => {
        settings[row.key] = row.value;
    });

    return settings;
}

// Install Paths functions
export function getInstallPaths(): InstallPath[] {
    if (!settingsDb) return [];

    const rows = settingsDb.prepare('SELECT id, path, lastUsed FROM install_paths ORDER BY lastUsed DESC').all() as any[];
    return rows.map(row => ({
        id: row.id,
        path: row.path,
        lastUsed: row.lastUsed
    }));
}

export function addInstallPath(installPath: string): { success: boolean; error?: string; id?: number } {
    if (!settingsDb) return { success: false, error: 'Database not initialized' };

    // Normalize path
    const normalizedPath = path.normalize(installPath);

    try {
        const result = settingsDb.prepare('INSERT INTO install_paths (path, lastUsed) VALUES (?, ?)').run(normalizedPath, Date.now());
        return { success: true, id: result.lastInsertRowid as number };
    } catch (error: any) {
        if (error.code === 'SQLITE_CONSTRAINT_UNIQUE') {
            return { success: false, error: 'Path already exists' };
        }
        return { success: false, error: error.message };
    }
}

export function removeInstallPath(id: number): { success: boolean } {
    if (!settingsDb) return { success: false };

    settingsDb.prepare('DELETE FROM install_paths WHERE id = ?').run(id);
    return { success: true };
}

export function updateLastUsedPath(installPath: string): void {
    if (!settingsDb) return;

    const normalizedPath = path.normalize(installPath);
    settingsDb.prepare('UPDATE install_paths SET lastUsed = ? WHERE path = ?').run(Date.now(), normalizedPath);
}

export function getMostRecentPath(): string | null {
    if (!settingsDb) return null;

    const row = settingsDb.prepare('SELECT path FROM install_paths ORDER BY lastUsed DESC LIMIT 1').get() as any;
    return row ? row.path : null;
}
