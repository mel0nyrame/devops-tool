#!/usr/bin/env node
const { spawnSync } = require('node:child_process');
const { dirname, join } = require('node:path');
const tool = '__TOOL__';
const platform = `${process.platform}-${process.arch}`;
const suffix = process.platform === 'win32' ? '.exe' : '';
let manifest;
try {
  manifest = require.resolve(`@mel0nyrame/devops-tool-${platform}/package.json`);
} catch {
  console.error(`${tool}: no binary for ${platform}; check platform support and install optional dependencies`);
  process.exit(1);
}
const result = spawnSync(join(dirname(manifest), tool + suffix), process.argv.slice(2), { stdio: 'inherit' });
if (result.error) console.error(result.error.message);
if (result.signal) process.kill(process.pid, result.signal);
process.exitCode = result.status ?? 1;
