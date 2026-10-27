import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

if (process.platform !== 'win32') process.exit(0);

const script = fileURLToPath(new URL('../src-tauri/native/game-hook/build.ps1', import.meta.url));
const requireAll = process.argv.includes('--require-all');
const result = spawnSync('powershell.exe', [
  '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', script,
  ...(requireAll ? ['-RequireAll'] : []),
], { cwd: path.dirname(script), stdio: 'inherit' });
if (result.status !== 0) process.exit(result.status ?? 1);
