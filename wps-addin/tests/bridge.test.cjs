const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../js/bridge.js'), 'utf8');
const events = ['SlideShowBegin', 'SlideShowEnd', 'SlideShowNextSlide',
  'SlideShowNextClick', 'SlideShowOnNext', 'SlideShowOnPrevious'];

function fixture({ ready = true, withEvents = true, failingEvent = null } = {}) {
  const requests = [], listeners = [], attempts = new Map();
  const timeouts = new Map(), intervals = new Map();
  let nextId = 0;
  const failures = new Set(failingEvent ? [failingEvent] : []);
  const api = {
    AddApiEventListener(event, callback) {
      attempts.set(event, (attempts.get(event) || 0) + 1);
      if (failures.has(event)) throw new Error('event API not ready');
      listeners.push({ event, callback });
    },
  };
  const app = {
    ActivePresentation: {
      SlideShowWindow: { View: { CurrentShowPosition: 3, GetClickIndex() { return 2; } } },
    },
  };
  if (withEvents) app.ApiEvent = api;
  const context = vm.createContext({
    window: { Application: ready ? app : undefined },
    fetch(url) {
      requests.push(url);
      return Promise.resolve({ text: () => Promise.resolve('') });
    },
    setTimeout(callback) { const id = ++nextId; timeouts.set(id, callback); return id; },
    clearTimeout(id) { timeouts.delete(id); },
    setInterval(callback) { const id = ++nextId; intervals.set(id, callback); return id; },
    encodeURIComponent,
  });
  function loadScript() { vm.runInContext(source, context); }
  loadScript();
  return {
    app, api, context, requests, listeners, attempts, failures, timeouts, intervals, loadScript,
    retry() {
      assert.equal(timeouts.size, 1, 'there must be exactly one pending retry');
      const [id, callback] = timeouts.entries().next().value;
      timeouts.delete(id);
      callback();
    },
    emit(event) {
      listeners.filter(listener => listener.event === event).forEach(listener => listener.callback());
    },
  };
}

function assertStartedOnce(f) {
  assert.deepEqual(f.listeners.map(listener => listener.event).sort(), [...events].sort());
  assert.equal(f.intervals.size, 1);
  assert.equal(f.timeouts.size, 0);
  assert.equal(f.requests.filter(url => url.endsWith('/hello?m=sidera-boot')).length, 1);
  assert.equal(f.requests.filter(url => url.endsWith('/hello?m=sidera-bridge')).length, 1);
  for (const event of events) {
    const before = f.requests.length;
    f.emit(event);
    assert.equal(f.requests.length, before + 1, `${event} should push exactly once`);
    assert.equal(new URL(f.requests.at(-1)).searchParams.get('m'), `EVENT ${event} pos=3 click=2`);
  }
}

test('repeated onLoad registers each event and starts polling only once', () => {
  const f = fixture();
  for (let i = 0; i < 10; i++) f.context.OnAddinLoad({});
  assertStartedOnce(f);
});

test('evaluating the script again in the same page does not duplicate listeners', () => {
  const f = fixture();
  f.context.startBridge();
  f.loadScript();
  f.context.OnAddinLoad({});
  assertStartedOnce(f);
});

test('waiting for Application schedules only one retry', () => {
  const f = fixture({ ready: false });
  for (let i = 0; i < 10; i++) f.context.startBridge();
  assert.equal(f.timeouts.size, 1);
  assert.equal(f.listeners.length, 0);
  assert.equal(f.intervals.size, 0);
  f.context.window.Application = f.app;
  f.retry();
  assertStartedOnce(f);
});

test('onLoad tolerates Application becoming available later', () => {
  const f = fixture({ ready: false });
  assert.equal(f.context.OnAddinLoad({}), true);
  f.context.window.Application = f.app;
  f.retry();
  assertStartedOnce(f);
});

test('polling continues while ApiEvent is unavailable', () => {
  const f = fixture({ withEvents: false });
  for (let i = 0; i < 10; i++) f.context.startBridge();
  assert.equal(f.timeouts.size, 1);
  assert.equal(f.intervals.size, 1);
  f.intervals.values().next().value();
  assert.equal(f.requests.filter(url => url.endsWith('/poll')).length, 1);
  f.app.ApiEvent = f.api;
  f.retry();
  assertStartedOnce(f);
});

test('partial registration retries only the failed event', () => {
  const f = fixture({ failingEvent: 'SlideShowNextSlide' });
  f.context.startBridge();
  assert.equal(f.listeners.length, 5);
  assert.equal(f.intervals.size, 1);
  f.failures.clear();
  f.retry();
  assertStartedOnce(f);
  for (const event of events) {
    assert.equal(f.attempts.get(event), event === 'SlideShowNextSlide' ? 2 : 1);
  }
});

test('persistent registration failures keep retries and polling bounded', () => {
  const f = fixture({ failingEvent: 'SlideShowBegin' });
  f.context.startBridge();
  for (let i = 0; i < 5; i++) {
    f.retry();
    assert.equal(f.listeners.length, 5);
    assert.equal(f.timeouts.size, 1);
    assert.equal(f.intervals.size, 1);
  }
  f.failures.clear();
  f.retry();
  assertStartedOnce(f);
  assert.equal(f.attempts.get('SlideShowBegin'), 7);
  assert.equal(f.attempts.get('SlideShowEnd'), 1);
});

test('manual startup after readiness cancels the pending retry', () => {
  const f = fixture({ withEvents: false });
  f.context.startBridge();
  assert.equal(f.timeouts.size, 1);
  f.app.ApiEvent = f.api;
  f.context.startBridge();
  assertStartedOnce(f);
});
