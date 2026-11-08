import fs from 'fs';
import path from 'path';

export interface ServerLaunchConfig {
    hostname: { enabled: boolean; value: string };
    identity: { enabled: boolean; value: string };
    port: { enabled: boolean; value: string };
    rconPort: { enabled: boolean; value: string };
    rconPassword: { enabled: boolean; value: string };
    rconWeb: { enabled: boolean };
    maxPlayers: { enabled: boolean; value: string };
    worldSize: { enabled: boolean; value: string };
    seed: { enabled: boolean; value: string };
    saveInterval: { enabled: boolean; value: string };
    customArgs: string;
}

export const DEFAULT_SERVER_CONFIG: ServerLaunchConfig = {
    hostname: { enabled: true, value: 'My Rust Server' },
    identity: { enabled: true, value: 'rustyserver' },
    port: { enabled: true, value: '28015' },
    rconPort: { enabled: true, value: '28016' },
    rconPassword: { enabled: true, value: 'changeme' },
    rconWeb: { enabled: true },
    maxPlayers: { enabled: true, value: '50' },
    worldSize: { enabled: true, value: '3000' },
    seed: { enabled: false, value: '12345' },
    saveInterval: { enabled: true, value: '300' },
    customArgs: '',
};

const CONFIG_FILENAME = '.rustyserver.json';
const BAT_FILENAME = 'RustServer.bat';

export function loadServerConfig(installPath: string): ServerLaunchConfig {
    const configPath = path.join(installPath, CONFIG_FILENAME);
    if (fs.existsSync(configPath)) {
        try {
            const saved = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
            return { ...DEFAULT_SERVER_CONFIG, ...saved };
        } catch {
            return DEFAULT_SERVER_CONFIG;
        }
    }
    return DEFAULT_SERVER_CONFIG;
}

export function saveServerConfig(installPath: string, config: ServerLaunchConfig): void {
    const configPath = path.join(installPath, CONFIG_FILENAME);
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
    generateBatFile(installPath, config);
}

/** Oxide drops its loader into an `oxide` folder at the server root once installed - no special launch arguments are required for it to load. */
export function detectOxide(installPath: string): boolean {
    return fs.existsSync(path.join(installPath, 'oxide'));
}

export function getModsFolderPath(installPath: string): string {
    return path.join(installPath, 'oxide');
}

function buildArgs(config: ServerLaunchConfig): string[] {
    const args = ['-batchmode', '+server.secure', '1'];

    if (config.hostname.enabled && config.hostname.value) args.push('+server.hostname', `"${config.hostname.value}"`);
    if (config.identity.enabled && config.identity.value) args.push('+server.identity', `"${config.identity.value}"`);
    if (config.port.enabled && config.port.value) args.push('+server.port', config.port.value);
    if (config.rconPort.enabled && config.rconPort.value) args.push('+rcon.port', config.rconPort.value);
    if (config.rconPassword.enabled && config.rconPassword.value) args.push('+rcon.password', `"${config.rconPassword.value}"`);
    if (config.rconWeb.enabled) args.push('+rcon.web', '1');
    if (config.maxPlayers.enabled && config.maxPlayers.value) args.push('+server.maxplayers', config.maxPlayers.value);
    if (config.worldSize.enabled && config.worldSize.value) args.push('+server.worldsize', config.worldSize.value);
    if (config.seed.enabled && config.seed.value) args.push('+server.seed', config.seed.value);
    if (config.saveInterval.enabled && config.saveInterval.value) args.push('+server.saveinterval', config.saveInterval.value);

    if (config.customArgs.trim()) {
        args.push(...config.customArgs.trim().split(/\s+/));
    }

    return args;
}

function generateBatFile(installPath: string, config: ServerLaunchConfig): void {
    const args = buildArgs(config);
    const batPath = path.join(installPath, BAT_FILENAME);
    const contents = `@echo off\r\ncd /d "%~dp0"\r\nRustDedicated.exe ${args.join(' ')}\r\npause\r\n`;
    fs.writeFileSync(batPath, contents);
}
