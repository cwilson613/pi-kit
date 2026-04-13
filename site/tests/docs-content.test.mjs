import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';

const here = dirname(fileURLToPath(import.meta.url));
const docsDir = resolve(here, '../src/pages/docs');

function readDoc(name) {
  return readFileSync(resolve(docsDir, name), 'utf8');
}

test('install docs separate stable public guidance from preview guidance', () => {
  const content = readDoc('install.astro');

  assert.match(content, /public site documents the stable channel only/i);
  assert.match(content, /preview site tracks staging guidance/i);
  assert.match(content, /go to <a href=\{previewSiteUrl\}>\{previewSiteUrl\}<\/a>/);
  assert.match(content, /CHANNEL=nightly/);
});

test('homepage differentiates stable public site from preview site', () => {
  const content = readFileSync(resolve(here, '../src/pages/index.astro'), 'utf8');

  assert.match(content, /Public stable site only\./);
  assert.match(content, /Preview channel:/);
  assert.match(content, /Nightly channel:/);
  assert.match(content, /public stable releases remain on/i);
});

test('providers docs call out stable vs preview split', () => {
  const content = readDoc('providers.astro');

  assert.match(content, /public stable provider surface/i);
  assert.match(content, /RC\/nightly/i);
});

test('site builds in stable and preview variants', () => {
  execFileSync('npm', ['run', 'build'], {
    cwd: resolve(here, '..'),
    env: { ...process.env, PUBLIC_SITE_VARIANT: 'stable', PUBLIC_SITE_URL: 'https://omegon.styrene.io' },
    stdio: 'pipe',
  });

  execFileSync('npm', ['run', 'build'], {
    cwd: resolve(here, '..'),
    env: {
      ...process.env,
      PUBLIC_SITE_VARIANT: 'preview',
      PUBLIC_SITE_URL: 'https://omegon.dev',
      PUBLIC_PREVIEW_SITE_URL: 'https://omegon.dev',
    },
    stdio: 'pipe',
  });
});
