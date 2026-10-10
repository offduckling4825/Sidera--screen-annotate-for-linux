const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../js/bridge.js'), 'utf8');
const settle = () => new Promise(resolve => setImmediate(resolve));

function fixture({ abort = true } = {}) {
  const requests = [], timeouts = new Map(), commands = [];
  const state = { pos: 1, click: 0 };
  let nextId = 0, throwFetch = false;
  const view = {
    get CurrentShowPosition() { return state.pos; },
    GetClickIndex() { return state.click; },
    Next() { commands.push('NEXT'); },
    Previous() { commands.push('PREV'); },
  };
  const context = vm.createContext({
    window: { Application: { ActivePresentation: {
      get SlideShowWindow() { return state.pos > 0 ? { View: view } : null; },
    } } },
    AbortController: abort ? AbortController : undefined,
    fetch(url, options) {
      const request = { url, options };
      requests.push(request);
      if (throwFetch) throw new Error('fetch unavailable');
      return new Promise((resolve, reject) => Object.assign(request, { resolve, reject }));
    },
    setTimeout(callback, delay) {
      const id = ++nextId;
      timeouts.set(id, { callback, delay });
      return id;
    },
    clearTimeout(id) { timeouts.delete(id); },
    encodeURIComponent,
  });
  function loadScript() { vm.runInContext(source, context); }
  loadScript();
  return {
    context, state, requests, timeouts, commands, loadScript,
    throwFetch(value) { throwFetch = value; },
    tick() { context.bridgePushIfChanged(); },
    async answer(index, body = 'OK', status = 200) {
      requests[index].resolve({ ok: status >= 200 && status < 300, text: () => Promise.resolve(body) });
      await settle();
    },
    async fail(index) {
      requests[index].reject(new Error('offline'));
      await settle();
    },
    expire() {
      assert.equal(timeouts.size, 1, 'one state request deadline must be pending');
      const [id, timer] = timeouts.entries().next().value;
      assert.equal(timer.delay, 3000);
      timeouts.delete(id);
      timer.callback();
    },
    message(index) { return new URL(requests[index].url).searchParams.get('m'); },
  };
}

test('two failed pushes retry the same page until an OK acknowledgement', async () => {
  const f = fixture();
  for (let i = 0; i < 2; i++) {
    f.tick();
    assert.equal(f.requests.length, i + 1);
    await f.fail(i);
    assert.equal(f.timeouts.size, 0);
  }
  f.tick();
  assert.equal(f.requests.length, 3);
  await f.answer(2, ' OK\n');
  for (let i = 0; i < 10; i++) f.tick();
  assert.equal(f.requests.length, 3, 'acknowledged unchanged state must not be resent');
  assert.equal(f.timeouts.size, 0);
  assert.ok(f.requests.every(request => request.options.cache === 'no-store'));
});

for (const [label, body, status] of [
  ['HTTP failure', 'OK', 500], ['unexpected response body', 'NEXT', 200],
]) {
  test(`${label} does not acknowledge state or execute a command`, async () => {
    const f = fixture();
    f.tick();
    await f.answer(0, body, status);
    f.tick();
    assert.equal(f.requests.length, 2);
    assert.deepEqual(f.commands, []);
  });
}

test('response body read failure permits another attempt', async () => {
  const f = fixture();
  f.tick();
  f.requests[0].resolve({ ok: true, text: () => Promise.reject(new Error('body interrupted')) });
  await settle();
  f.tick();
  assert.equal(f.requests.length, 2);
});

test('only one state request is pending and rapid changes coalesce to the latest page', async () => {
  const f = fixture();
  f.tick();
  for (let pos = 2; pos <= 10; pos++) {
    f.state.pos = pos;
    f.state.click = pos;
    f.tick();
  }
  assert.equal(f.requests.length, 1);
  await f.answer(0);
  f.tick();
  assert.equal(f.requests.length, 2);
  assert.equal(f.message(1), 'EVENT SlideShowState pos=10 click=10');
  await f.answer(1);
  f.tick();
  assert.equal(f.requests.length, 2);
});

test('ambiguous failure forces resynchronization even after returning to the last confirmed page', async () => {
  const f = fixture();
  f.tick();
  await f.answer(0);
  f.state.pos = 2;
  f.tick();
  f.state.pos = 1;
  await f.fail(1); // The server may have applied page 2 before its response was lost.
  f.tick();
  assert.equal(f.requests.length, 3);
  assert.equal(f.message(2), 'EVENT SlideShowState pos=1 click=0');
});

test('synchronous fetch errors do not leave state permanently pending', async () => {
  const f = fixture();
  f.throwFetch(true);
  f.tick();
  f.throwFetch(false);
  f.tick();
  assert.equal(f.requests.length, 2);
  await f.answer(1);
  f.tick();
  assert.equal(f.requests.length, 2);
  assert.equal(f.timeouts.size, 0);
});

for (const abort of [true, false]) {
  test(`timed out requests cannot acknowledge or clear a newer request (AbortController=${abort})`, async () => {
    const f = fixture({ abort });
    f.tick();
    f.expire();
    if (abort) assert.equal(f.requests[0].options.signal.aborted, true);
    f.state.pos = 2;
    f.tick();
    assert.equal(f.requests.length, 2);
    await f.answer(0); // A late response after cancellation must have no effect.
    f.state.pos = 3;
    f.tick();
    assert.equal(f.requests.length, 2, 'new request must remain pending');
    assert.equal(f.timeouts.size, 1);
    await f.fail(1);
    f.tick();
    assert.equal(f.message(2), 'EVENT SlideShowState pos=3 click=0');
    await f.answer(2);
    f.tick();
    assert.equal(f.requests.length, 3);
    assert.equal(f.timeouts.size, 0);
  });
}

test('the deadline also covers a response body that never completes', async () => {
  const f = fixture();
  f.tick();
  f.requests[0].resolve({ ok: true, text: () => new Promise(() => {}) });
  await settle();
  f.expire();
  f.tick();
  assert.equal(f.requests.length, 2);
  await f.answer(1);
  assert.equal(f.timeouts.size, 0);
});

test('same-page script reload preserves pending and confirmed state', async () => {
  const f = fixture();
  f.tick();
  f.loadScript();
  f.tick();
  assert.equal(f.requests.length, 1);
  await f.answer(0);
  f.loadScript();
  f.tick();
  assert.equal(f.requests.length, 1);
  f.state.click++;
  f.tick();
  assert.equal(f.message(1), 'EVENT SlideShowState pos=1 click=1');
});

test('missing presentation or Application sends no state request', () => {
  const f = fixture();
  f.state.pos = -1;
  f.tick();
  f.context.window.Application = undefined;
  f.tick();
  assert.equal(f.requests.length, 0);
  assert.equal(f.timeouts.size, 0);
});

test('successful poll commands still run while failed HTTP responses do not', async () => {
  const f = fixture();
  f.context.bridgeFetch('/poll');
  await f.answer(0, 'NEXT');
  f.context.bridgeFetch('/poll');
  await f.answer(1, 'PREV');
  f.context.bridgeFetch('/poll');
  await f.answer(2, 'NEXT', 503);
  assert.deepEqual(f.commands, ['NEXT', 'PREV']);
});
