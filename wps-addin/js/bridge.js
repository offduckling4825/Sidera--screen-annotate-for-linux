// ============================================================
// Sidera联动桥（加载项内运行）
//   · 监听 WPS 放映事件 → 上报批注 app (127.0.0.1:16666 /push)
//   · 轮询 /poll 取 START/NEXT/PREV/EXIT/GOTO，通过 WPS 接口执行
// 配合 app 的“WPS 联动”使用：批注缓存按真实页号驱动，
// 页内动画步不动批注。
// ============================================================
var BRIDGE_URL = 'http://127.0.0.1:16666';
var BRIDGE_START_COMMAND = 'SlideShowFromCurrent';

function OnAddinLoad(ribbonUI) {
    var app = bridgeApplication()
    if (ribbonUI && app && typeof app.ribbonUI != 'object') app.ribbonUI = ribbonUI
    startBridge()
    return true
}

function OnSaPing() {
    var st = bridgeState()
    alert('批注联动桥在线\npos=' + st.pos + ' click=' + st.click)
    return true
}

function bridgeApplication() {
    try {
        if (window.Application) return window.Application
        if (window.wps && typeof window.wps.WppApplication == 'function')
            return window.wps.WppApplication()
        return window.wps ? window.wps.Application : null
    } catch (e) { return null }
}
function bridgeShowContext() {
    try {
        var app = bridgeApplication()
        if (!app) return null
        var a = app.ActivePresentation
        if (!a && app.Presentations && app.Presentations.Count > 0)
            a = app.Presentations.Item(1)
        var sw = a ? a.SlideShowWindow : null
        if (sw) return { window: sw, presentation: a }
        if (app.SlideShowWindows && app.SlideShowWindows.Count > 0) {
            sw = app.SlideShowWindows.Item(1)
            var deck = null
            try { deck = sw.Presentation } catch (e) {}
            return { window: sw, presentation: deck }
        }
        return null
    } catch (e) { return null }
}
function bridgeView() {
    try {
        var context = bridgeShowContext()
        return context ? context.window.View : null
    } catch (e) { return null }
}
function bridgePageState(context, view) {
    var result = { page: -1, total: -1 }
    function positive(value) {
        var n = Number(value)
        return isFinite(n) && n > 0 && n <= 2147483647 && Math.floor(n) === n ? n : -1
    }
    try {
        var app = bridgeApplication()
        var deck = context ? context.presentation : app && app.ActivePresentation
        if (!deck) return result
        try {
            var count = Number(deck.Slides.Count)
            if (count === 0) return result
            result.total = positive(count)
        } catch (e) {}
        // Use the actual deck index, not the order within a custom slideshow.
        try {
            var current = view || (!context && app.ActiveWindow && app.ActiveWindow.View)
            result.page = positive(current && current.Slide && current.Slide.SlideIndex)
        } catch (e) {}
        if (result.total > 0 && result.page > result.total) result.page = -1
    } catch (e) {}
    return result
}
function bridgeState() {
    var st = { pos: -1, click: -1, ready: 0, page: -1, total: -1, jump: 0, deck: -1 }
    try {
        var context = bridgeShowContext()
        var v = context ? context.window.View : null
        var pages = bridgePageState(context, v)
        st.page = pages.page; st.total = pages.total
        var jump = bridgeJumpTarget()
        if (jump) { st.jump = 1; st.deck = jump.epoch }
        if (!v) { st.ready = bridgeStartTarget() ? 1 : 0; return st }
        var pos = v.CurrentShowPosition
        var click = (typeof v.GetClickIndex == 'function') ? v.GetClickIndex() : -1
        st.pos = pos; st.click = click
    } catch (e) {}
    return st
}
function bridgeJumpTarget() {
    try {
        var app = bridgeApplication()
        var context = bridgeShowContext()
        var deck = context ? context.presentation : app && app.ActivePresentation
        var view = context ? context.window.View : app && app.ActiveWindow && app.ActiveWindow.View
        if (!deck || !view || typeof view.GotoSlide !== 'function') return null
        var pages = bridgePageState(context, context ? view : null)
        if (!(pages.page > 0 && pages.total >= pages.page)) return null
        var name = String(deck.FullName || deck.Name || '')
        if (!name) return null
        // Bind a queued jump to the active document and editing/showing session.
        // A replacement document must not receive an old wheel selection.
        var key = name + '\n' + pages.total + '\n' + (context ? 'show' : 'edit')
        var identity = window._sideraDeckIdentity
        if (!identity || identity.key !== key) {
            identity = { key: key, epoch: identity ? identity.epoch % 2147483646 + 1 : Date.now() % 2147483646 + 1 }
            window._sideraDeckIdentity = identity
        }
        return { view: view, total: pages.total, epoch: identity.epoch }
    } catch (e) { return null }
}
function bridgeStartTarget() {
    try {
        var app = bridgeApplication()
        // 只启动当前文档；没有活动文档时不猜测列表里的第一份文件。
        var presentation = app && app.ActivePresentation
        if (!presentation || !presentation.Slides || !(Number(presentation.Slides.Count) > 0)) return null
        if (app.SlideShowWindows && app.SlideShowWindows.Count > 0) return null
        var bars = app.CommandBars
        if (!bars || typeof bars.ExecuteMso != 'function') return null
        if (typeof bars.GetEnabledMso == 'function' && !bars.GetEnabledMso(BRIDGE_START_COMMAND)) return null
        return bars
    } catch (e) { return null }
}
function bridgeStateMessage(name, st) {
    var message = 'EVENT ' + name + ' pos=' + st.pos + ' click=' + st.click + ' ready=' + st.ready
    if (st.page > 0 || st.total > 0) message += ' page=' + st.page + ' total=' + st.total
    if (st.jump === 1 && st.deck > 0) message += ' jump=1 deck=' + st.deck
    return message
}
function bridgeFetch(path, onResult, signal, onText) {
    try {
        var options = { cache: 'no-store' }
        if (signal) options.signal = signal
        fetch(BRIDGE_URL + path, options)
            .then(function (r) {
                if (!r.ok) throw new Error('Bridge HTTP request failed')
                return r.text()
            })
            .then(function (t) {
                var c = (t || '').trim()
                if (onText) onText(c)
                else if (onResult) onResult(c === 'OK')
                else if (path.split('?')[0] === '/poll' && c) bridgeCmd(c)
            })
            .catch(function () { if (onResult) onResult(false) })
    } catch (e) { if (onResult) onResult(false) }
}
function bridgeCmd(cmd) {
    var jump = /^GOTO ([1-9][0-9]{0,9}) ([1-9][0-9]{0,9})$/.exec(cmd)
    if (!jump && cmd !== 'START' && cmd !== 'NEXT' && cmd !== 'PREV' && cmd !== 'EXIT') return
    try {
        var v = bridgeView()
        if (jump) {
            var destination = bridgeJumpTarget()
            var page = Number(jump[1]), epoch = Number(jump[2])
            if (!destination || destination.epoch !== epoch || page > destination.total)
                throw new Error('Page selection no longer belongs to the active document')
            destination.view.GotoSlide(page)
        } else if (cmd == 'START') {
            var target = !v && bridgeStartTarget()
            if (!target || Date.now() < (window._sideraStartingUntil || 0))
                throw new Error('No presentation ready to start')
            window._sideraStartingUntil = Date.now() + 3000
            // 使用 WPS 自身的“当页开始”，执行时读取当前页；不改文档放映范围。
            target.ExecuteMso(BRIDGE_START_COMMAND)
        } else {
            if (!v) throw new Error('No slideshow')
            if (cmd == 'NEXT') v.Next()
            else if (cmd == 'PREV') v.Previous()
            else v.Exit()
        }
        bridgeFetch('/push?m=' + encodeURIComponent('COMMAND_OK ' + cmd))
        if (jump) bridgePushIfChanged()
    } catch (e) {
        bridgeFetch('/push?m=' + encodeURIComponent('COMMAND_FAILED ' + cmd))
    }
}
function bridgePoll() {
    if (window._sideraPollInFlight) return
    var st = bridgeState()
    var controller = typeof AbortController == 'function' ? new AbortController() : null
    var pending = { timer: null }
    window._sideraPollInFlight = pending
    var finish = function () {
        if (window._sideraPollInFlight !== pending) return false
        clearTimeout(pending.timer)
        window._sideraPollInFlight = null
        return true
    }
    pending.timer = setTimeout(function () {
        finish()
        if (controller) controller.abort()
    }, 1500)
    bridgeFetch('/poll?m=' + encodeURIComponent(bridgeStateMessage('SlideShowState', st)),
        finish, controller ? controller.signal : null, function (cmd) {
            // 已过期轮询的迟到回复不得在下一次放映中执行。
            if (finish() && cmd) bridgeCmd(cmd)
        })
}
var BridgeSlideEvents = ['SlideShowBegin', 'SlideShowEnd', 'SlideShowNextSlide', 'SlideShowNextClick', 'SlideShowOnNext', 'SlideShowOnPrevious'];
// 轮询式状态上报：解决“上一页”没有后续新页号事件的问题
// （WPS 的 Next 会补发 NextClick/NextSlide，但 Previous 常只有 OnPrevious 且带旧 pos）
// 仅记录收到成功回复的快照；同页重载脚本时保留确认状态和在途请求。
var _lastState = window._sideraLastState || (window._sideraLastState = { pos: -1, click: -1, ready: 0, page: -1, total: -1, jump: 0, deck: -1 });
function bridgePushIfChanged() {
    try {
        if (window._sideraStateInFlight) return
        var st = bridgeState()
        if (st.pos !== _lastState.pos || st.click !== _lastState.click || st.ready !== _lastState.ready ||
            st.page !== _lastState.page || st.total !== _lastState.total || st.jump !== _lastState.jump || st.deck !== _lastState.deck) {
            var controller = typeof AbortController == 'function' ? new AbortController() : null
            var pending = { timer: null }
            window._sideraStateInFlight = pending
            var finish = function (ok) {
                // 超时请求的迟到回复不能确认旧页，也不能清除较新的在途请求。
                if (window._sideraStateInFlight !== pending) return
                clearTimeout(pending.timer)
                window._sideraStateInFlight = null
                // 失败时服务端可能已处理请求；即使回到上次确认页，也必须重新同步。
                _lastState.pos = ok ? st.pos : -1
                _lastState.click = ok ? st.click : -1
                _lastState.ready = ok ? st.ready : -1
                _lastState.page = ok ? st.page : -1
                _lastState.total = ok ? st.total : -1
                _lastState.jump = ok ? st.jump : -1
                _lastState.deck = ok ? st.deck : -1
            }
            pending.timer = setTimeout(function () {
                finish(false)
                if (controller) controller.abort()
            }, 3000)
            bridgeFetch('/push?m=' + encodeURIComponent(bridgeStateMessage('SlideShowState', st)),
                finish, controller ? controller.signal : null)
        }
    } catch (e) {}
}
function scheduleBridgeStart() {
    if (window._sideraStartRetry != null) return
    window._sideraStartRetry = setTimeout(function () {
        window._sideraStartRetry = null
        startBridge()
    }, 500)
}
function startBridge() {
    // 手动重试或重复 onLoad 取代尚未执行的重试，避免定时器不断累积。
    if (window._sideraStartRetry != null) {
        clearTimeout(window._sideraStartRetry)
        window._sideraStartRetry = null
    }
    // 启动探针：只要本 JS 被执行，就先报到一次，便于诊断
    try {
        if (!window._sideraBootPinged) {
            window._sideraBootPinged = true
            bridgeFetch('/hello?m=sidera-boot')
        }
    } catch (e) {}
    var app = bridgeApplication()
    if (!app) {
        scheduleBridgeStart()               // 等 Application 就绪
        return
    }
    // 事件监听为“可选增强”：即使没有 ApiEvent，也要启动轮询上报真实页号
    // 状态保存在 window 上，同一页面重复执行脚本时也不重复注册。
    var handlers = window._sideraEventHandlers || (window._sideraEventHandlers = {})
    var eventsReady = true
    try {
        var api = app.ApiEvent || (window.wps && window.wps.ApiEvent)
        if (api) {
            for (var i = 0; i < BridgeSlideEvents.length; i++) {
                (function (ev) {
                    if (handlers[ev]) return
                    try {
                        var handler = function () {
                            var st = bridgeState()
                            bridgeFetch('/push?m=' + encodeURIComponent(bridgeStateMessage(ev, st)))
                        }
                        api.AddApiEventListener(ev, handler)
                        handlers[ev] = handler
                    } catch (e) { eventsReady = false }
                })(BridgeSlideEvents[i])
            }
        } else {
            eventsReady = false
        }
    } catch (e) { eventsReady = false }
    if (!eventsReady) scheduleBridgeStart()
    // 轮询同时带当前状态：重启批注程序也能恢复页号，不依赖 WPS 事件顺序。
    if (!window._sideraTimerStarted) {
        window._sideraTimerStarted = true
        bridgeFetch('/hello?m=sidera-bridge')
        setInterval(function () {
            bridgePoll()
            bridgePushIfChanged()              // 状态变化即上报（前后翻都覆盖）
        }, 300)
    }
}
