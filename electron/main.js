"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var electron_1 = require("electron");
var path_1 = require("path");
var url_1 = require("url");
var better_sqlite3_1 = require("better-sqlite3");
var __dirname = path_1.default.dirname((0, url_1.fileURLToPath)(import.meta.url));
// Handle creating/removing shortcuts on Windows when installing/uninstalling.
if (require('electron-squirrel-startup')) {
    electron_1.app.quit();
}
var mainWindow = null;
var db = null;
function createWindow() {
    mainWindow = new electron_1.BrowserWindow({
        width: 1200,
        height: 800,
        backgroundColor: '#281C1C',
        webPreferences: {
            preload: path_1.default.join(__dirname, 'preload.js'),
            nodeIntegration: false,
            contextIsolation: true,
        },
    });
    if (process.env.NODE_ENV === 'development') {
        mainWindow.loadURL('http://localhost:5173');
        mainWindow.webContents.openDevTools();
    }
    else {
        mainWindow.loadFile(path_1.default.join(__dirname, '../dist/index.html'));
    }
}
function initDatabase() {
    var dbPath = path_1.default.join(electron_1.app.getPath('userData'), 'rusty_launcher.db');
    db = new better_sqlite3_1.default(dbPath);
    db.pragma('journal_mode = WAL');
    // Create versions table
    db.exec("\n    CREATE TABLE IF NOT EXISTS versions (\n      id INTEGER PRIMARY KEY AUTOINCREMENT,\n      manifestId TEXT NOT NULL,\n      depotId TEXT NOT NULL,\n      buildDate TEXT,\n      installPath TEXT NOT NULL,\n      isEacEnabled INTEGER DEFAULT 1\n    )\n  ");
}
electron_1.app.whenReady().then(function () {
    initDatabase();
    createWindow();
    electron_1.app.on('activate', function () {
        if (electron_1.BrowserWindow.getAllWindows().length === 0) {
            createWindow();
        }
    });
});
electron_1.app.on('window-all-closed', function () {
    if (process.platform !== 'darwin') {
        electron_1.app.quit();
    }
});
// IPC Handlers
electron_1.ipcMain.handle('get-versions', function () {
    if (!db)
        return [];
    return db.prepare('SELECT * FROM versions').all();
});
