const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');

function loadFormatter() {
  const html = readFileSync(__dirname + '/dashboard.html', 'utf8');
  const start = html.indexOf('function escapeHtml(s) {');
  const end = html.indexOf('// ── Prompt sending', start);
  if (start === -1 || end === -1) {
    throw new Error('formatter block not found in dashboard.html');
  }

  const script = html.slice(start, end);
  const context = {};
  vm.createContext(context);
  vm.runInContext(script, context);
  return context;
}

describe('dashboard formatter', () => {
  it('renders markdown tables as HTML tables', () => {
    const { formatText } = loadFormatter();
    const input = [
      '## codebase_search: `foo`',
      '',
      '| File | Lines | Type | Score | Preview |',
      '|------|-------|------|-------|---------|',
      '| `src/app.rs` | 10-20 | code | 45.38 | fn render() |',
      '| `src/lib.rs` | 1-9 | code | 11.20 | helper |',
    ].join('\n');

    const html = formatText(input, 'tool');
    assert.match(html, /<table class="md-table">/);
    assert.match(html, /<th>File<\/th>/);
    assert.match(html, /<td><code>src\/app\.rs<\/code><\/td>/);
    assert.doesNotMatch(html, /\| File \| Lines \| Type \|/);
  });

  it('preserves fenced code blocks while formatting surrounding prose', () => {
    const { formatText } = loadFormatter();
    const input = [
      'before',
      '```rs',
      'fn main() {}',
      '```',
      '',
      '| A | B |',
      '|---|---|',
      '| 1 | 2 |',
    ].join('\n');

    const html = formatText(input, 'tool');
    assert.match(html, /<pre><code>fn main\(\) \{\}\n<\/code><\/pre>/);
    assert.match(html, /<table class="md-table">/);
    assert.match(html, /before/);
  });
});
