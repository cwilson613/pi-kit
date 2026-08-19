import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { parseSnippetYaml } from '../scripts/load-snippets.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const docsDir = resolve(here, '../src/pages/docs');
const snippetsDir = resolve(here, '../snippets');
const npmRunBuild = process.platform === 'win32'
  ? { command: process.env.ComSpec ?? 'cmd.exe', args: ['/d', '/s', '/c', 'npm run build'] }
  : { command: 'npm', args: ['run', 'build'] };
const buildEnvironment = { ...process.env, FORCE_COLOR: '0' };
if (process.platform === 'win32') {
  buildEnvironment.CI = 'true';
}

function readDoc(name) {
  return readFileSync(resolve(docsDir, name), 'utf8');
}

test('install docs use canonical snippets for all channels', () => {
  const content = readDoc('install.astro');

  // Uses snippet system, not hardcoded commands
  assert.match(content, /snippet\("install\.quick_install"\)/);
  assert.match(content, /snippet\("install\.install_nightly"\)/);
  assert.match(content, /snippet\("install\.install_version"\)/);
  assert.match(content, /snippet\("install\.verify_companions"\)/);
  assert.match(content, /snippet\("verify\.release_verify"\)/);
  assert.match(content, /href="\/docs\/recovery"/);
  assert.doesNotMatch(content, /omegon\.styrene\.dev/);
  // Auth commands use correct form
  assert.match(content, /snippet\("auth\.login_anthropic"\)/);
  assert.doesNotMatch(content, /omegon login(?! )/);
});

test('recovery docs consume canonical maintenance snippets', () => {
  const content = readDoc('recovery.astro');
  const required = [
    'identity',
    'doctor',
    'composition_inspect',
    'contribution_list_project',
    'contribution_disable_dry_run',
    'session_list',
    'session_quarantine_dry_run',
    'resource_list',
    'resource_prune_dry_run',
    'audit_verify',
  ];
  for (const key of required) {
    assert.match(content, new RegExp(`snippet\\("maintenance\\.${key}"\\)`));
  }
  assert.match(content, /snippet\("verify\.release_verify"\)/);
  assert.doesNotMatch(content, /omegon-maintain (fix|repair|clean|update|rollback)(\s|<)/);

  const layout = readFileSync(resolve(here, '../src/layouts/Docs.astro'), 'utf8');
  assert.match(layout, /href: "\/docs\/recovery"/);
});

test('release verification snippets do not trust arbitrary certificate identities', () => {
  const verification = readFileSync(resolve(snippetsDir, 'verify.yaml'), 'utf8');
  assert.doesNotMatch(verification, /certificate-identity-regexp\s+['"]?\.\*/);
  assert.match(verification, /omegon-maintain --json release verify/);
});

test('public docs contain no destructive root deletion example', () => {
  for (const page of readdirSync(docsDir).filter(f => f.endsWith('.astro'))) {
    assert.doesNotMatch(readDoc(page), /rm -rf \/ --no-preserve-root/);
  }
});

test('homepage has version selector and install section', () => {
  const content = readFileSync(resolve(here, '../src/pages/index.astro'), 'utf8');

  assert.match(content, /version-select/);
  assert.match(content, /Stable/);
  assert.match(content, /Nightly/);
  assert.match(content, /latestNightly/);
  assert.match(content, /data-tag=\{nightlyTag\}/);
  assert.match(content, /data-url=\{nightlyUrl\}/);
  assert.match(content, /--channel=nightly/);
  assert.match(content, /install-cmd/);
  assert.match(content, /copy-btn/);
  assert.doesNotMatch(content, /omegon\.styrene\.dev/);
});

test('privacy page uses canonical site label', () => {
  const content = readFileSync(resolve(here, '../src/pages/privacy.astro'), 'utf8');

  assert.match(content, /siteLabel/);
  assert.match(content, /omegon\.styrene\.io/);
});

test('extensions page uses extension init, not extension new', () => {
  const content = readDoc('extensions.astro');

  assert.match(content, /snippet\("cli\.extension_init"\)/);
  assert.doesNotMatch(content, /extension new/);
});

test('no page imports siteVariant', () => {
  const pages = readdirSync(docsDir).filter(f => f.endsWith('.astro'));
  for (const page of pages) {
    const content = readDoc(page);
    assert.doesNotMatch(content, /siteVariant/, `${page} still imports siteVariant`);
  }
});

test('snippet parser preserves multiline commands with CRLF input', () => {
  const parsed = parseSnippetYaml([
    'dev_clone_build:',
    '  cmd: |',
    '    git clone https://github.com/styrene-lab/omegon.git',
    '    cd omegon',
    '  desc: "Clone and build"',
    '',
  ].join('\r\n'));

  assert.deepEqual(parsed.dev_clone_build, {
    cmd: 'git clone https://github.com/styrene-lab/omegon.git\ncd omegon',
    desc: 'Clone and build',
  });
});

test('site builds successfully', () => {
  execFileSync(npmRunBuild.command, npmRunBuild.args, {
    cwd: resolve(here, '..'),
    env: buildEnvironment,
    stdio: 'pipe',
  });

  const changelogHtml = readFileSync(resolve(here, '../dist/changelog/index.html'), 'utf8');
  const privacyHtml = readFileSync(resolve(here, '../dist/privacy/index.html'), 'utf8');
  const termsHtml = readFileSync(resolve(here, '../dist/terms/index.html'), 'utf8');
  const installHtml = readFileSync(resolve(here, '../dist/docs/install/index.html'), 'utf8');
  const recoveryHtml = readFileSync(resolve(here, '../dist/docs/recovery/index.html'), 'utf8');
  const securityHtml = readFileSync(resolve(here, '../dist/docs/security/index.html'), 'utf8');
  const rootChangelog = readFileSync(resolve(here, '../../CHANGELOG.md'), 'utf8');

  assert.match(rootChangelog, /^\+\+\+/);
  for (const rendered of [changelogHtml, privacyHtml, termsHtml]) {
    assert.doesNotMatch(rendered, /imported_reference/);
    assert.doesNotMatch(rendered, /\[publication\]/);
    assert.doesNotMatch(rendered, /^\+\+\+/m);
    assert.doesNotMatch(rendered, /^---$/m);
  }
  assert.match(changelogHtml, /local sandbox evidence-substrate smoke suite/);
  assert.match(installHtml, /omegon-maintain --json identity/);
  assert.match(recoveryHtml, /omegon-maintain --json composition inspect/);
  assert.match(recoveryHtml, /contribution disable plugin:formatter --scope project/);
  assert.match(securityHtml, /omegon-maintain --json release verify/);
  for (const rendered of [installHtml, recoveryHtml, securityHtml]) {
    assert.doesNotMatch(rendered, /certificate-identity-regexp[^<]*\.\*/);
  }
  for (const version of [
    '0.19.6',
    '0.19.5',
    '0.19.4',
    '0.19.3',
    '0.19.2',
    '0.19.1',
    '0.19.0',
    '0.18.6',
    '0.18.5',
  ]) {
    assert.match(changelogHtml, new RegExp(`\\[${version}\\]`));
  }
});
