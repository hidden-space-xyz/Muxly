import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function commitBump(subject) {
  const match = /^([a-z]+)(?:\([^\r\n()]+\))?(!)?: .+/.exec(subject);
  if (!match) return 0;
  if (match[2]) return 3;
  if (match[1] === 'feat') return 2;
  return ['fix', 'perf'].includes(match[1]) ? 1 : 0;
}

export function nextVersion(version, bump) {
  const [major, minor, patch] = version.split('.').map(Number);
  if (bump === 3) return `${major + 1}.0.0`;
  if (bump === 2) return `${major}.${minor + 1}.0`;
  if (bump === 1) return `${major}.${minor}.${patch + 1}`;
  return version;
}

export function resolveRelease(cwd = process.cwd()) {
  const git = (...args) => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();
  const tags = git('tag', '--list', 'v*', '--sort=-version:refname').split('\n');
  const stable = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
  const previous = tags.find((tag) => stable.test(tag)) ?? '';
  if (previous) {
    git('merge-base', '--is-ancestor', previous, 'HEAD');
  }
  const version = previous ? previous.slice(1) : '0.0.0';
  const range = previous ? `${previous}..HEAD` : 'HEAD';
  const subjects = git('log', '--format=%s', range).split('\n');
  const bump = subjects.reduce((highest, subject) => Math.max(highest, commitBump(subject)), 0);
  const next = nextVersion(version, bump);
  return { changed: bump > 0, version: next, tag: `v${next}`, previous };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const release = resolveRelease();
  const repository = process.env.GITHUB_REPOSITORY;
  if (!repository || !process.env.GITHUB_OUTPUT) throw new Error('GitHub Actions environment required');
  const outputs = { ...release, image: `ghcr.io/${repository.toLowerCase()}` };
  appendFileSync(process.env.GITHUB_OUTPUT, Object.entries(outputs).map(([key, value]) => `${key}=${value}\n`).join(''));
  console.log(release.changed ? `Release ${release.tag} (previous: ${release.previous || 'none'})` : 'No release');
}
