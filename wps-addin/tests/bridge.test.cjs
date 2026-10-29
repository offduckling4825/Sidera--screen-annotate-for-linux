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
    window: { _sideraFullMode: true, _sideraAllowPoll: true, _sideraChannel: 'poll', Application: ready ? app : undefined },
    fetch(url) {
      requests.push(url);
      return Promise.resolve({ ok: true, text: () => Promise.resolve('OK') });
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
    assert.equal(new URL(f.requests.at(-1)).searchParams.get('m'), `EVENT ${event} pos=3 click=2 ready=0`);
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

test('replaced event API gets listeners and obsolete callbacks stop publishing', () => {
  const f = fixture();
  f.context.startBridge();
  const replacement = [];
  f.app.ApiEvent = { AddApiEventListener(event, callback) { replacement.push({ event, callback }); } };
  f.intervals.values().next().value();
  assert.equal(replacement.length, events.length);
  const before = f.requests.length;
  f.emit('SlideShowBegin');
  assert.equal(f.requests.length, before, 'obsolete API must not publish a new Begin');
  replacement.find(listener => listener.event === 'SlideShowBegin').callback();
  assert.equal(f.requests.length, before + 1);
  f.context.startBridge();
  assert.equal(replacement.length, events.length);
  assert.equal(f.intervals.size, 1);
});

test('onLoad tolerates Application becoming available later', () => {
  const f = fixture({ ready: false });
  assert.equal(f.context.OnAddinLoad({}), true);
  f.context.window.Application = f.app;
  f.retry();
  assertStartedOnce(f);
});

test('polling continues while ApiEvent is unavailable', async () => {
  const f = fixture({ withEvents: false });
  for (let i = 0; i < 10; i++) f.context.startBridge();
  assert.equal(f.timeouts.size, 1);
  assert.equal(f.intervals.size, 1);
  f.intervals.values().next().value();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.requests.filter(url => new URL(url).pathname === '/poll').length, 1);
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

test('WppApplication and wps.ApiEvent work without a global Application alias', () => {
  const f = fixture({ ready: false, withEvents: false });
  f.context.window.wps = { WppApplication: () => f.app, ApiEvent: f.api };
  f.context.OnAddinLoad({});
  assertStartedOnce(f);
  assert.equal(f.context.bridgeState().pos, 3);
});

test('absolute page jump works in show and edit views and rejects a replaced document', () => {
  for (const showing of [false, true]) {
    const f = fixture();
    const calls = [];
    const view = { Slide: { SlideIndex: 3 }, CurrentShowPosition: 3,
      GotoSlide(page) { calls.push(page); this.Slide.SlideIndex = page; } };
    const deck = { FullName: '/home/teacher/lesson.dps', Slides: { Count: 24 } };
    if (showing) deck.SlideShowWindow = { View: view };
    else f.app.ActiveWindow = { View: view };
    f.app.ActivePresentation = deck;
    const before = f.context.bridgeState();
    assert.equal(before.jump, 1);
    f.context.bridgeCmd(`GOTO 10 ${before.deck}`);
    assert.deepEqual(calls, [10]);
    f.context.bridgeCmd(`GOTO 25 ${before.deck}`);
    assert.deepEqual(calls, [10], 'out-of-range jumps must not reach WPS');
    deck.FullName = '/home/teacher/another-lesson.dps';
    f.context.bridgeCmd(`GOTO 8 ${before.deck}`);
    assert.deepEqual(calls, [10], 'a delayed command must not jump a new document');
    const current = f.context.bridgeState();
    assert.notEqual(current.deck, before.deck);
    f.context.bridgeCmd(`GOTO 8 ${current.deck}`);
    assert.deepEqual(calls, [10, 8]);
  }
});

test('SSE opens one event stream and handles mode and commands', () => {
  const requests = [], commands = [];
  let eventSource = null;
  class FakeEventSource {
    constructor(url) { this.url = url; eventSource = this; }
    close() { this.closed = true; }
  }
  const view = {
    CurrentShowPosition: 1,
    GetClickIndex() { return 0; },
    Next() { commands.push('NEXT'); },
  };
  const app = {
    ActivePresentation: { SlideShowWindow: { View: view } },
    ApiEvent: { AddApiEventListener() {} },
  };
  const context = vm.createContext({
    window: { _sideraFullMode: true, _sideraAllowPoll: true, _sideraChannel: 'poll', Application: app },
    EventSource: FakeEventSource,
    fetch(url) {
      requests.push(url);
      return Promise.resolve({ ok: true, text: () => Promise.resolve('OK') });
    },
    setTimeout() { return 1; },
    clearTimeout() {},
    encodeURIComponent,
  });
  vm.runInContext(source, context);
  context.startBridge();
  assert.ok(eventSource && eventSource.url.endsWith('/events'));
  eventSource.onopen();
  eventSource.onmessage({ data: 'MODE default' });
  eventSource.onmessage({ data: 'NEXT' });
  assert.deepEqual(commands, ['NEXT']);
  assert.equal(context.BRIDGE_FULL, false);
});

test('WebSocket is the primary channel and commands use the same socket', () => {
  const requests = [], commands = [];
  class FakeWebSocket {
    static instances = [];
    constructor(url) { this.url = url; this.readyState = 0; this.sent = []; this.listeners = {}; FakeWebSocket.instances.push(this); }
    addEventListener(type, callback) { (this.listeners[type] || (this.listeners[type] = [])).push(callback); }
    send(message) { this.sent.push(message); }
    close() { this.readyState = 3; }
    open() { this.readyState = 1; if (this.onopen) this.onopen(); (this.listeners.open || []).forEach(callback => callback()); }
    message(data) { (this.listeners.message || []).forEach(callback => callback({ data })); }
  }
  const view = {
    CurrentShowPosition: 1,
    GetClickIndex() { return 0; },
    Next() { commands.push('NEXT'); },
  };
  const app = {
    ActivePresentation: { SlideShowWindow: { View: view }, Slides: { Count: 20 } },
    ApiEvent: { AddApiEventListener() {} },
  };
  const context = vm.createContext({
    window: { _sideraFullMode: true, _sideraAllowPoll: false, _sideraChannel: 'none', Application: app },
    WebSocket: FakeWebSocket,
    fetch(url) {
      requests.push(url);
      return Promise.resolve({ ok: true, text: () => Promise.resolve('OK') });
    },
    setTimeout() { return 1; },
    clearTimeout() {},
    encodeURIComponent,
  });
  vm.runInContext(source, context);
  context.startBridge();
  const socket = FakeWebSocket.instances[0];
  assert.ok(socket && socket.url.endsWith('/ws'));
  socket.open();
  socket.message('MODE full');
  socket.message('NEXT');
  assert.deepEqual(commands, ['NEXT']);
  assert.equal(requests.filter(url => url.includes('/push')).length, 0,
    'WS mode must not use HTTP /push');
  assert.ok(socket.sent.some(message => message.startsWith('EVENT ')));
  assert.ok(socket.sent.some(message => message.startsWith('COMMAND_OK')));
});
