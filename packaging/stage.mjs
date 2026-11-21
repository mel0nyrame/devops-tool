// Stage one npm package per name/version from the native Release assets.
import { copyFile, chmod, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const [version, assets, output] = process.argv.slice(2);
const scope = '@melonyrame/';
const repository = { type: 'git', url: 'git+https://github.com/mel0nyrame/devops-tool.git' };
const tools = ['fsz', 'rmem'];
const platforms = [['linux', 'x64'], ['darwin', 'arm64'], ['win32', 'x64']];
const optionalDependencies = {};
const template = await readFile(new URL('./launch.cjs', import.meta.url), 'utf8');

async function stage(name, fields) {
  const dir = join(output, name);
  await mkdir(dir, { recursive: true });
  await writeFile(join(dir, 'package.json'), JSON.stringify({ name: scope + name, version, license: 'MIT', repository, ...fields }, null, 2));
  return dir;
}

for (const [os, cpu] of platforms) {
  const platform = `${os}-${cpu}`;
  const name = `devops-tool-${platform}`;
  const dir = await stage(name, { os: [os], cpu: [cpu], ...(os === 'linux' && { libc: ['glibc'] }) });
  const suffix = os === 'win32' ? '.exe' : '';
  for (const tool of tools) {
    const target = join(dir, tool + suffix);
    await copyFile(join(assets, `${tool}-${platform}${suffix}`), target);
    await chmod(target, 0o755);
  }
  optionalDependencies[scope + name] = version;
}

for (const [name, commands] of [['devops-tool', tools], ...tools.map(tool => [tool, [tool]])]) {
  const bin = Object.fromEntries(commands.map(tool => [tool, `${tool}.cjs`]));
  const dir = await stage(name, { bin, optionalDependencies });
  for (const tool of commands) {
    await writeFile(join(dir, bin[tool]), template.replace('__TOOL__', tool), { mode: 0o755 });
  }
}
