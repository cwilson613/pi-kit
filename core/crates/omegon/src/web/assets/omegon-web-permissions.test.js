const { it } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');

it('reconnect snapshots restore one actionable permission and clear settled prompts', async () => {
  function element() {
    return {
      children: [], dataset: {}, style: {}, listeners: {},
      appendChild(child) { this.children.push(child); child.parent = this; },
      addEventListener(name, handler) { this.listeners[name] = handler; },
      remove() { this.parent.children = this.parent.children.filter(child => child !== this); },
      querySelectorAll() { return this.children.filter(child => child.dataset.permissionId); },
    };
  }
  const transcript = element();
  const actions = [];
  const context = {
    transcript,
    document: { createElement: element },
    clearEmpty() {}, scrollToBottom() {},
    postAction: async action => { actions.push(action); return { status: 'accepted' }; },
  };
  const html = readFileSync(__dirname + '/omegon-web.html', 'utf8');
  const blocks = [
    ['function renderPermissionPrompt(', '// Interactive operator-wait'],
    ['function applySnapshot(', '// ── Live stream'],
  ].map(([start, end]) => {
    const from = html.indexOf(start);
    const to = html.indexOf(end, from);
    assert.ok(from >= 0 && to > from);
    return html.slice(from, to);
  });
  vm.createContext(context);
  vm.runInContext(blocks.join('\n'), context);
  const permission = { request_id: 'perm-1', tool_name: 'bash', path: '/tmp/work' };
  const snapshot = { surfaces: { command: { pending_permissions: [permission] } } };
  context.applySnapshot(snapshot);
  assert.equal(transcript.children.length, 1);
  context.renderPermissionPrompt('perm-1', 'bash', '/tmp/work');
  assert.equal(transcript.children.length, 1, 'snapshot/live overlap must not duplicate approval');
  context.applySnapshot(snapshot);
  assert.equal(transcript.children.length, 1, 'repeated snapshot must be idempotent');
  const box = transcript.children[0];
  assert.match(box.children[0].textContent, /bash.*\/tmp\/work/);
  await box.children[1].children[0].listeners.click();
  assert.equal(actions.length, 1);
  assert.equal(actions[0].request_id, 'perm-1');
  assert.equal(actions[0].allow, true);
  context.applySnapshot({ surfaces: { command: { pending_permissions: [] } } });
  assert.equal(transcript.children.length, 0, 'settled approval must disappear on reconciliation');
});
