#!/usr/bin/env node
// Thin wrapper around the Tauri CLI that loads optional, git-ignored local
// environment files (`.env.local`, then `.env`) before invoking `tauri`.
//
// Why: the Tauri CLI reads settings like `APPLE_SIGNING_IDENTITY` from the
// process environment but does NOT itself load `.env` files. Keeping the
// signing identity in a git-ignored `.env.local` lets local builds sign
// automatically while the committed config stays clean and portable — a
// contributor or CI without that file just gets an unsigned/ad-hoc build.
//
// Format: `KEY=VALUE` per line, `#` comments and a leading `export ` allowed,
// surrounding single/double quotes stripped. Real environment variables
// always win over file values.

import { spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

for (const file of ['.env.local', '.env']) {
  if (!existsSync(file)) continue;
  for (const raw of readFileSync(file, 'utf8').split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const match = line.match(/^(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/);
    if (!match) continue;
    const [, key, rawValue] = match;
    if (process.env[key] !== undefined) continue; // real env wins
    let value = rawValue.trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    process.env[key] = value;
  }
}

const child = spawn('tauri', process.argv.slice(2), {
  stdio: 'inherit',
  shell: process.platform === 'win32', // tauri.cmd on Windows needs a shell
});
child.on('exit', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 0);
});
child.on('error', (err) => {
  console.error(err);
  process.exit(1);
});
