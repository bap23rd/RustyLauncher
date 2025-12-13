"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var electron_1 = require("electron");
electron_1.contextBridge.exposeInMainWorld('electronAPI', {
    getVersions: function () { return electron_1.ipcRenderer.invoke('get-versions'); },
});
