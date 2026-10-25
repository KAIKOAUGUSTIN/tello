// Testes do script de versão: node --test scripts/

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';

import {
  bumpVersion,
  compareVersions,
  computeNext,
  parseCommit,
  prependChangelog,
  renderNotes,
  setVersion,
} from './version.mjs';

const dirs = [];
after(() => dirs.forEach((d) => rmSync(d, { recursive: true, force: true })));

function git(cwd, ...args) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

/** Repositório mínimo com os mesmos manifestos do projeto. */
function repo(version = '0.1.0') {
  const cwd = mkdtempSync(join(tmpdir(), 'tello-version-'));
  dirs.push(cwd);
  mkdirSync(join(cwd, 'src-tauri'));
  writeFileSync(
    join(cwd, 'Cargo.toml'),
    `[workspace]\nmembers = ["app"]\n\n[workspace.package]\nedition = "2021"\nversion = "${version}"\nauthors = ["Alguém"]\n\n[workspace.dependencies]\nfoo = { version = "1" }\n`,
  );
  writeFileSync(
    join(cwd, 'Cargo.lock'),
    `# gerado\nversion = 4\n\n[[package]]\nname = "app"\nversion = "${version}"\ndependencies = [\n "dep",\n]\n\n[[package]]\nname = "dep"\nversion = "${version}"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n`,
  );
  writeFileSync(join(cwd, 'package.json'), `{\n  "name": "app",\n  "version": "${version}"\n}\n`);
  writeFileSync(
    join(cwd, 'package-lock.json'),
    `${JSON.stringify({ name: 'app', version, lockfileVersion: 3, packages: { '': { name: 'app', version }, 'node_modules/x': { version } } }, null, 2)}\n`,
  );
  writeFileSync(join(cwd, 'src-tauri/tauri.conf.json'), `{\n  "productName": "App",\n  "version": "${version}"\n}\n`);
  git(cwd, 'init', '-q', '-b', 'main');
  git(cwd, 'config', 'user.name', 'Teste');
  git(cwd, 'config', 'user.email', 'teste@example.com');
  git(cwd, 'config', 'commit.gpgsign', 'false');
  git(cwd, 'config', 'tag.gpgsign', 'false');
  git(cwd, 'add', '-A');
  git(cwd, 'commit', '-q', '-m', 'Primeiro commit');
  return cwd;
}

let n = 0;
function commit(cwd, message, { empty = false } = {}) {
  if (!empty) writeFileSync(join(cwd, `arquivo-${++n}.txt`), message);
  git(cwd, 'add', '-A');
  git(cwd, 'commit', '-q', ...(empty ? ['--allow-empty'] : []), '-m', message);
}

describe('versões', () => {
  it('sobe patch, minor e major', () => {
    assert.equal(bumpVersion('1.2.3', 'patch'), '1.2.4');
    assert.equal(bumpVersion('1.2.3', 'minor'), '1.3.0');
    assert.equal(bumpVersion('1.2.3', 'major'), '2.0.0');
  });

  it('compara numericamente', () => {
    assert.ok(compareVersions('0.10.0', '0.9.9') > 0);
    assert.equal(compareVersions('1.0.0', '1.0.0'), 0);
  });
});

describe('commits', () => {
  it('lê o nível de cada mensagem', () => {
    assert.equal(parseCommit('fix: corrige vídeo').level, 'patch');
    assert.equal(parseCommit('feat(ui): modo escuro').level, 'minor');
    assert.equal(parseCommit('feat!: novo protocolo').level, 'major');
    assert.equal(parseCommit('refactor: troca API', 'BREAKING CHANGE: remove X').level, 'major');
    assert.equal(parseCommit('Suporte a iOS [minor]').level, 'minor');
    assert.equal(parseCommit('Reescreve tudo', 'detalhes\n[major]').level, 'major');
    assert.equal(parseCommit('Ajusta textos').level, 'patch');
  });

  it('aceita mensagens livres sem confundir com tipos', () => {
    const c = parseCommit('CI: testes no Ubuntu');
    assert.equal(c.type, null);
    assert.equal(c.description, 'CI: testes no Ubuntu');
    assert.equal(parseCommit('Novo layout [minor]').description, 'Novo layout');
  });
});

describe('próxima versão', () => {
  it('sem tag, a primeira release é a versão dos arquivos', () => {
    const cwd = repo('0.1.0');
    commit(cwd, 'feat: tudo');
    const next = computeNext(cwd);
    assert.equal(next.version, '0.1.0');
    assert.equal(next.commit, false);
    assert.equal(next.release, true);
  });

  it('patch, minor e major conforme os commits desde a tag', () => {
    const cwd = repo('0.1.0');
    git(cwd, 'tag', 'v0.1.0');
    commit(cwd, 'Corrige o vídeo');
    assert.equal(computeNext(cwd).version, '0.1.1');
    commit(cwd, 'feat: modo retrato');
    assert.equal(computeNext(cwd).version, '0.2.0');
    commit(cwd, 'Protocolo novo [major]');
    const next = computeNext(cwd);
    assert.equal(next.version, '1.0.0');
    assert.equal(next.commit, true);
    assert.equal(next.commits.length, 3);
  });

  it('usa a maior tag, não a mais recente', () => {
    const cwd = repo('0.1.0');
    git(cwd, 'tag', 'v0.9.0');
    commit(cwd, 'x');
    git(cwd, 'tag', 'v0.10.0');
    commit(cwd, 'fix: y');
    assert.equal(computeNext(cwd).previous, '0.10.0');
    assert.equal(computeNext(cwd).version, '0.10.1');
  });

  it('respeita a versão subida à mão nos arquivos', () => {
    const cwd = repo('0.1.0');
    git(cwd, 'tag', 'v0.1.0');
    setVersion(cwd, '0.5.0');
    commit(cwd, 'Versão 0.5.0');
    const next = computeNext(cwd);
    assert.equal(next.version, '0.5.0');
    assert.equal(next.commit, false);
  });

  it('não lança release sem mudanças, nem por commits de release ou vazios', () => {
    const cwd = repo('0.1.0');
    git(cwd, 'tag', 'v0.1.0');
    assert.equal(computeNext(cwd).release, false);
    commit(cwd, 'chore(release): v0.1.0');
    commit(cwd, 'Commit vazio', { empty: true });
    assert.equal(computeNext(cwd).release, false);
  });

  it('o salto pode ser forçado (workflow_dispatch)', () => {
    const cwd = repo('0.1.0');
    git(cwd, 'tag', 'v0.1.0');
    commit(cwd, 'fix: x');
    assert.equal(computeNext(cwd, { bump: 'minor' }).version, '0.2.0');
    assert.throws(() => computeNext(cwd, { bump: 'enorme' }));
  });
});

describe('aplicar a versão', () => {
  it('atualiza todos os manifestos, menos as dependências de terceiros', () => {
    const cwd = repo('0.1.0');
    setVersion(cwd, '1.4.2');
    const read = (f) => readFileSync(join(cwd, f), 'utf8');
    assert.match(read('Cargo.toml'), /\[workspace\.package\]\nedition = "2021"\nversion = "1\.4\.2"/);
    assert.match(read('Cargo.toml'), /foo = \{ version = "1" \}/);
    assert.match(read('Cargo.lock'), /name = "app"\nversion = "1\.4\.2"/);
    assert.match(read('Cargo.lock'), /name = "dep"\nversion = "0\.1\.0"/);
    assert.equal(JSON.parse(read('package.json')).version, '1.4.2');
    const lock = JSON.parse(read('package-lock.json'));
    assert.equal(lock.version, '1.4.2');
    assert.equal(lock.packages[''].version, '1.4.2');
    assert.equal(lock.packages['node_modules/x'].version, '0.1.0');
    assert.equal(JSON.parse(read('src-tauri/tauri.conf.json')).version, '1.4.2');
  });

  it('recusa versões inválidas', () => {
    assert.throws(() => setVersion(repo(), 'v1.0'));
  });
});

describe('notas e changelog', () => {
  it('agrupa por seção com links', () => {
    const commits = [
      { sha: 'a'.repeat(40), ...parseCommit('feat(ui): botão de foto') },
      { sha: 'b'.repeat(40), ...parseCommit('fix: vídeo travando') },
      { sha: 'c'.repeat(40), ...parseCommit('deps: bump tauri') },
      { sha: 'd'.repeat(40), ...parseCommit('Ajusta README') },
    ];
    const notes = renderNotes({ version: '0.2.0', previous: '0.1.0', commits }, 'eu/tello', new Date('2026-01-02'));
    assert.match(notes, /^## \[0\.2\.0\]\(https:\/\/github\.com\/eu\/tello\/compare\/v0\.1\.0\.\.\.v0\.2\.0\) \(2026-01-02\)/);
    assert.match(notes, /### Novidades\n\n- \*\*ui:\*\* Botão de foto \(\[aaaaaaa\]\(https:\/\/github\.com\/eu\/tello\/commit\/a{40}\)\)/);
    assert.match(notes, /### Correções\n\n- Vídeo travando/);
    assert.match(notes, /### Outras mudanças\n\n- Ajusta README/);
    assert.match(notes, /### Dependências\n\n- Bump tauri/);
    assert.ok(notes.indexOf('Novidades') < notes.indexOf('Correções'));
  });

  it('o changelog mais novo fica no topo', () => {
    const cwd = repo();
    prependChangelog(cwd, '## 0.1.0 (2026-01-01)\n\n- Primeira versão.\n');
    prependChangelog(cwd, '## 0.2.0 (2026-02-01)\n\n- Segunda.\n');
    const log = readFileSync(join(cwd, 'CHANGELOG.md'), 'utf8');
    assert.match(log, /^# Changelog\n/);
    assert.ok(log.indexOf('## 0.2.0') < log.indexOf('## 0.1.0'));
    assert.equal(log.match(/# Changelog/g).length, 1);
  });
});
