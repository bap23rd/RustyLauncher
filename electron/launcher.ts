import { spawn } from 'child_process';
import { shell } from 'electron';
import fs from 'fs';
import path from 'path';

export class GameLauncher {
    /**
     * Launch Rust with EAC enabled by running Rust.exe directly
     */
    launchWithEAC(installPath: string): boolean {
        const rustExePath = path.join(installPath, 'Rust.exe');

        if (!fs.existsSync(rustExePath)) {
            console.error('Rust.exe not found at:', rustExePath);
            return false;
        }

        console.log('Launching Rust.exe with EAC...');

        try {
            spawn(rustExePath, [], {
                cwd: installPath,
                detached: true,
                stdio: 'ignore'
            }).unref();
            return true;
        } catch (error) {
            console.error('Failed to launch Rust.exe:', error);
            return false;
        }
    }

    /**
     * Launch Rust without EAC by running RustClient.exe directly
     */
    launchWithoutEAC(installPath: string): boolean {
        const rustClientPath = path.join(installPath, 'RustClient.exe');

        if (!fs.existsSync(rustClientPath)) {
            console.error('RustClient.exe not found at:', rustClientPath);
            return false;
        }

        console.log('Launching RustClient.exe without EAC...');

        try {
            spawn(rustClientPath, [], {
                cwd: installPath,
                detached: true,
                stdio: 'ignore'
            }).unref();
            return true;
        } catch (error) {
            console.error('Failed to launch RustClient.exe:', error);
            return false;
        }
    }

    /**
     * Check if RustClient.exe exists in the install directory
     */
    checkEACClientExists(installPath: string): boolean {
        const rustClientPath = path.join(installPath, 'RustClient.exe');
        return fs.existsSync(rustClientPath);
    }

    /**
     * Check if Rust.exe exists in the install directory
     */
    checkRustExeExists(installPath: string): boolean {
        const rustExePath = path.join(installPath, 'Rust.exe');
        return fs.existsSync(rustExePath);
    }
}
