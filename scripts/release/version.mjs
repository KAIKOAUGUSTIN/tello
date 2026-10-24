#!/usr/bin/env node
// Versão automática do Tello Pilot (Node puro, sem dependências).
//
//   node scripts/release/version.mjs next [--bump auto|patch|minor|major] [--notes release-notes.md]
//       Calcula a próxima versão a partir da última tag vX.Y.Z e dos commits
//       desde ela, e grava as notas da release. No GitHub Actions também
//       escreve os valores em $GITHUB_OUTPUT.
//   node scripts/release/version.mjs set <versão>
//       Aplica a versão em Cargo.toml, Cargo.lock, package.json,
//       package-lock.json e src-tauri/tauri.conf.json.
//   node scripts/release/version.mjs changelog <versão> [--notes release-notes.md]
//       Acrescenta as notas no topo do CHANGELOG.md.
//
// Regras do salto de versão (o maior vence):
//   major: "tipo!: ...", "BREAKING CHANGE" no corpo ou [major] na mensagem;
//   minor: "feat: ..." ou [minor] na mensagem;
//   patch: todo o resto.

import { execFileSync } from 'node:child_process';
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/;
const RELEASE_COMMIT = /^chore\(release\)/;
const LEVELS = ['patch', 'minor', 'major'];
// Só tipos conhecidos (em minúsculas): "CI: testes..." continua sendo texto livre.
const CONVENTIONAL = /^(feat|fix|perf|refactor|docs|style|test|build|ci|chore|revert|deps)(?:\(([^)]*)\))?(!)?:\s*(.+)$/;

// ---------------------------------------------------------------- versões

export function parseVersion(v) {
  const m = SEMVER.exec(v);
  if (!m) throw new Error(`versão inválida: ${v}`);
  return [Number(m[1]), Number(m[2]), Number(m[3])];
}

export function compareVersions(a, b) {
  const [x, y] = [parseVersion(a), parseVersion(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
}

export function bumpVersion(v, level) {
  const [major, minor, patch] = parseVersion(v);
  if (level === 'major') return `${major + 1}.0.0`;
  if (level === 'minor') return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

// ---------------------------------------------------------------- commits

/** Lê um commit no formato do Conventional Commits, aceitando mensagens livres. */
export function parseCommit(subject, body = '') {
  const m = CONVENTIONAL.exec(subject);
  const type = m ? m[1] : null;
  const scope = m?.[2]?.toLowerCase() ?? null;
  const text = `${subject}\n${body}`;
  let level = 'patch';
  if (type === 'feat' || /\[minor\]/i.test(text)) level = 'minor';
  if (m?.[3] || /BREAKING[ -]CHANGE/.test(body) || /\[major\]/i.test(text)) level = 'major';
  const description = (m ? m[4] : subject).replace(/\s*\[(major|minor)\]\s*/gi, ' ').trim();
  return { type, scope, level, description, breaking: level === 'major' };
}

function sectionOf(c) {
  if (c.breaking) return 'breaking';
  if (c.type === 'feat') return 'feat';
  if (c.type === 'fix') return 'fix';
  if (c.type === 'deps' || c.scope === 'deps' || (c.type === 'ci' && /^bump /i.test(c.description))) return 'deps';
  return 'other';
}

const SECTIONS = [
  ['breaking', 'Mudanças incompatíveis'],
  ['feat', 'Novidades'],
  ['fix', 'Correções'],
  ['other', 'Outras mudanças'],
  ['deps', 'Dependências'],
];

// ---------------------------------------------------------------- git

function git(cwd, ...args) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

/** Maior tag vX.Y.Z alcançável a partir do HEAD. */
export function lastTag(cwd) {
  const tags = git(cwd, 'tag', '--merged', 'HEAD', '--list', 'v*')
    .split('\n')
    .filter((t) => SEMVER.test(t.slice(1)));
  tags.sort((a, b) => compareVersions(b.slice(1), a.slice(1)));
  return tags[0] ?? null;
}

/** Commits desde `tag` (ou desde o início), sem merges, releases e commits vazios. */
export function commitsSince(cwd, tag) {
  const range = tag ? [`${tag}..HEAD`] : ['HEAD'];
  const raw = git(cwd, 'log', '--no-merges', '--format=%H%x1f%s%x1f%b%x1e', ...range);
  return raw
    .split('\x1e')
    .map((r) => r.trim())
    .filter(Boolean)
    .map((r) => {
      const [sha, subject, body = ''] = r.split('\x1f');
      return { sha, subject, body: body.trim() };
    })
    .filter((c) => !RELEASE_COMMIT.test(c.subject))
    .filter((c) => git(cwd, 'diff-tree', '--no-commit-id', '--name-only', '-r', '--root', c.sha) !== '')
    .map((c) => ({ ...c, ...parseCommit(c.subject, c.body) }));
}

/** owner/repo, do GitHub Actions ou do remote "origin". */
export function repoSlug(cwd) {
  if (process.env.GITHUB_REPOSITORY) return process.env.GITHUB_REPOSITORY;
  try {
    const url = git(cwd, 'remote', 'get-url', 'origin');
    return /github\.com[:/](.+?)(?:\.git)?$/.exec(url)?.[1] ?? null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------- próxima versão

export function fileVersion(cwd) {
  return JSON.parse(readFileSync(join(cwd, 'package.json'), 'utf8')).version;
}

/**
 * Decide a próxima release. Sem tag nenhuma, a primeira release é a versão dos
 * arquivos; se alguém subiu a versão nos arquivos à mão, ela é respeitada.
 */
export function computeNext(cwd, { bump = 'auto' } = {}) {
  const current = fileVersion(cwd);
  const tag = lastTag(cwd);
  const previous = tag ? tag.slice(1) : null;
  const commits = commitsSince(cwd, tag);
  if (bump !== 'auto' && !LEVELS.includes(bump)) throw new Error(`--bump inválido: ${bump}`);

  let version;
  let level = null;
  if (!previous) {
    version = current;
  } else if (compareVersions(current, previous) > 0) {
    version = current;
  } else if (commits.length === 0 && bump === 'auto') {
    return { release: false, version: previous, previous, commits, level, commit: false };
  } else {
    level =
      bump === 'auto'
        ? commits.reduce((acc, c) => (LEVELS.indexOf(c.level) > LEVELS.indexOf(acc) ? c.level : acc), 'patch')
        : bump;
    version = bumpVersion(previous, level);
  }
  return { release: true, version, previous, commits, level, commit: version !== current };
}

// ---------------------------------------------------------------- notas

export function renderNotes({ version, previous, commits }, slug, date = new Date()) {
  const day = date.toISOString().slice(0, 10);
  const base = slug ? `https://github.com/${slug}` : null;
  const title = base && previous ? `[${version}](${base}/compare/v${previous}...v${version})` : version;
  const out = [`## ${title} (${day})`, ''];

  if (commits.length === 0) out.push(previous ? '- Nova versão sem mudanças de código.' : '- Primeira versão.', '');
  for (const [id, name] of SECTIONS) {
    const list = commits.filter((c) => sectionOf(c) === id);
    if (list.length === 0) continue;
    out.push(`### ${name}`, '');
    for (const c of list) {
      const short = c.sha.slice(0, 7);
      const link = base ? `[${short}](${base}/commit/${c.sha})` : short;
      const scope = c.scope && c.scope !== 'deps' ? `**${c.scope}:** ` : '';
      out.push(`- ${scope}${capitalize(c.description)} (${link})`);
    }
    out.push('');
  }
  return out.join('\n');
}

/** Tabela de downloads, igual aos nomes gerados pelo .github/workflows/build.yml. */
export function renderDownloads(version) {
  const f = (suffix) => `\`Tello-Pilot_${version}_${suffix}\``;
  return [
    '### Downloads',
    '',
    '| Plataforma | Arquivo |',
    '| --- | --- |',
    `| Windows 10/11 | ${f('windows_x64-setup.exe')} |`,
    `| macOS 11+ (Intel e Apple Silicon) | ${f('macos_universal.dmg')} |`,
    `| Linux | ${f('linux_amd64.AppImage')} ou ${f('linux_amd64.deb')} |`,
    `| Android 7+ | ${f('android_universal.apk')} |`,
    `| iOS 16.4+ (sideload) | ${f('ios_unsigned.ipa')} |`,
    '',
    'O `tello-cli` (diagnóstico pelo terminal) também está anexado para Windows, macOS e Linux.',
    'Como instalar em cada plataforma: veja a seção **Instalação** do README.',
    '',
  ].join('\n');
}

function capitalize(s) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

// ---------------------------------------------------------------- aplicar a versão

/** Atualiza a versão em todos os manifestos (sem precisar do cargo). */
export function setVersion(cwd, version) {
  parseVersion(version);
  const edit = (file, fn) => {
    const path = join(cwd, file);
    const before = readFileSync(path, 'utf8');
    const after = fn(before);
    if (after === before && !before.includes(version)) throw new Error(`versão não encontrada em ${file}`);
    writeFileSync(path, after);
  };

  const old = fileVersion(cwd);

  edit('Cargo.toml', (s) =>
    s.replace(/(\[workspace\.package\](?:\n(?!\[)[^\n]*)*?\nversion\s*=\s*")[^"]*(")/, `$1${version}$2`),
  );
  // No Cargo.lock só os pacotes do workspace (sem "source") mudam de versão.
  edit('Cargo.lock', (s) =>
    s
      .split('\n[[package]]\n')
      .map((block) =>
        /\nsource = /.test(block) || !new RegExp(`\nversion = "${escape(old)}"`).test(`\n${block}`)
          ? block
          : block.replace(`version = "${old}"`, `version = "${version}"`),
      )
      .join('\n[[package]]\n'),
  );
  edit('package.json', (s) => s.replace(/("version":\s*")[^"]*(")/, `$1${version}$2`));
  edit('package-lock.json', (s) => {
    const lock = JSON.parse(s);
    lock.version = version;
    if (lock.packages?.['']) lock.packages[''].version = version;
    return `${JSON.stringify(lock, null, 2)}\n`;
  });
  edit('src-tauri/tauri.conf.json', (s) => s.replace(/("version":\s*")[^"]*(")/, `$1${version}$2`));
}

function escape(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function prependChangelog(cwd, notes) {
  const path = join(cwd, 'CHANGELOG.md');
  const header = '# Changelog\n\nTodas as mudanças do Tello Pilot. Gerado automaticamente a cada release.\n';
  const existing = existsSync(path) ? readFileSync(path, 'utf8') : '';
  const i = existing.indexOf('\n## ');
  const older = i >= 0 ? existing.slice(i + 1).trimEnd() : '';
  writeFileSync(path, `${header}\n${notes.trimEnd()}\n${older ? `\n${older}\n` : ''}`);
}

// ---------------------------------------------------------------- linha de comando

function arg(args, name, fallback) {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
}

function output(values) {
  const file = process.env.GITHUB_OUTPUT;
  if (!file) return;
  for (const [k, v] of Object.entries(values)) appendFileSync(file, `${k}=${v}\n`);
}

function main(argv) {
  const [cmd, ...args] = argv;
  const cwd = resolve(arg(args, '--cwd', process.cwd()));
  const notesFile = resolve(cwd, arg(args, '--notes', 'release-notes.md'));

  if (cmd === 'next') {
    const next = computeNext(cwd, { bump: arg(args, '--bump', 'auto') });
    if (next.release) {
      const notes = `${renderNotes(next, repoSlug(cwd))}\n${renderDownloads(next.version)}`;
      writeFileSync(notesFile, notes);
    }
    const summary = {
      release: next.release,
      version: next.version,
      tag: `v${next.version}`,
      previous: next.previous ?? '',
      bump: next.level ?? (next.release ? 'manual' : 'none'),
      commit: next.commit,
      commits: next.commits.length,
    };
    output(summary);
    console.log(JSON.stringify(summary, null, 2));
    return;
  }
  if (cmd === 'set') {
    const v = args[0];
    if (!v) throw new Error('uso: version.mjs set <versão>');
    setVersion(cwd, v);
    console.log(`versão ${v} aplicada`);
    return;
  }
  if (cmd === 'changelog') {
    const v = args[0];
    if (!v) throw new Error('uso: version.mjs changelog <versão>');
    const notes = readFileSync(notesFile, 'utf8');
    // O CHANGELOG guarda só as mudanças; a tabela de downloads fica na release.
    prependChangelog(cwd, notes.split('\n### Downloads')[0]);
    console.log(`CHANGELOG.md atualizado com a ${v}`);
    return;
  }
  throw new Error('uso: version.mjs next|set|changelog (veja o cabeçalho do arquivo)');
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`erro: ${e.message}`);
    process.exit(1);
  }
}
