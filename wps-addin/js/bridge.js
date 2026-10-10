// ============================================================
// Sidera联动桥（加载项内运行）
//   · 监听 WPS 放映事件 → 上报批注 app (127.0.0.1:16666 /push)
//   · 轮询 /poll 取 NEXT/PREV 指令（保留，默认 app 用虚拟键推进）
// 配合 app 的“WPS 接口调试模式”使用：批注缓存按真实页号驱动，
// 页内动画步不动批注。
// ============================================================
var BRIDGE_URL = 'http://127.0.0.1:16666';

function OnAddinLoad(ribbonUI) {
    if (ribbonUI && window.Application && typeof window.Application.ribbonUI != 'object')
        window.Application.ribbonUI = ribbonUI
    startBridge()
    return true
}

function OnSaPing() {
    var st = bridgeState()
    alert('批注联动桥在线\npos=' + st.pos + ' click=' + st.click)
    return true
}

function bridgeView() {
    try {
        var app = window.Application
        if (!app) return null
        var a = app.ActivePresentation
        if (!a && app.Presentations && app.Presentations.Count > 0)
            a = app.Presentations.Item(1)
        var sw = a ? a.SlideShowWindow : null
        if (!sw && app.SlideShowWindows && app.SlideShowWindows.Count > 0)
            sw = app.SlideShowWindows.Item(1)
        return sw ? sw.View : null
    } catch (e) { return null }
}
function bridgeState() {
    try {
        var v = bridgeView()
        if (!v) return { pos: -1, click: -1 }
        var pos = v.CurrentShowPosition
        var click = (typeof v.GetClickIndex == 'function') ? v.GetClickIndex() : -1
        return { pos: pos, click: click }
    } catch (e) { return { pos: -1, click: -1 } }
}
function bridgeFetch(path, onResult, signal) {
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
                if (onResult) onResult(c === 'OK')
                else if (c) bridgeCmd(c)
            })
            .catch(function () { if (onResult) onResult(false) })
    } catch (e) { if (onResult) onResult(false) }
}
function bridgeCmd(cmd) {
    try {
        var v = bridgeView()
        if (!v) return                       // 非放映状态不动作
        if (cmd == 'NEXT') v.Next()
        else if (cmd == 'PREV') v.Previous()
    } catch (e) {}
}
var BridgeSlideEvents = ['SlideShowBegin', 'SlideShowEnd', 'SlideShowNextSlide', 'SlideShowNextClick', 'SlideShowOnNext', 'SlideShowOnPrevious'];
// 轮询式状态上报：解决“上一页”没有后续新页号事件的问题
// （WPS 的 Next 会补发 NextClick/NextSlide，但 Previous 常只有 OnPrevious 且带旧 pos）
// 仅记录收到成功回复的快照；同页重载脚本时保留确认状态和在途请求。
var _lastState = window._sideraLastState || (window._sideraLastState = { pos: -1, click: -1 });
function bridgePushIfChanged() {
    try {
        if (window._sideraStateInFlight) return
        var st = bridgeState()
        if (st.pos < 1) return                       // 不在放映
        if (st.pos !== _lastState.pos || st.click !== _lastState.click) {
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
            }
            pending.timer = setTimeout(function () {
                finish(false)
                if (controller) controller.abort()
            }, 3000)
            bridgeFetch('/push?m=' + encodeURIComponent('EVENT SlideShowState pos=' + st.pos + ' click=' + st.click),
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
    if (typeof window.Application == 'undefined' || !window.Application) {
        scheduleBridgeStart()               // 等 Application 就绪
        return
    }
    // 事件监听为“可选增强”：即使没有 ApiEvent，也要启动轮询上报真实页号
    // 状态保存在 window 上，同一页面重复执行脚本时也不重复注册。
    var handlers = window._sideraEventHandlers || (window._sideraEventHandlers = {})
    var eventsReady = true
    try {
        var api = window.Application.ApiEvent
        if (api) {
            for (var i = 0; i < BridgeSlideEvents.length; i++) {
                (function (ev) {
                    if (handlers[ev]) return
                    try {
                        var handler = function () {
                            var st = bridgeState()
                            bridgeFetch('/push?m=' + encodeURIComponent('EVENT ' + ev + ' pos=' + st.pos + ' click=' + st.click))
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
    // 轮询 /poll 取 NEXT/PREV；并每 300ms 上报状态变化（点屏幕翻页靠这个覆盖）
    if (!window._sideraTimerStarted) {
        window._sideraTimerStarted = true
        bridgeFetch('/hello?m=sidera-bridge')
        setInterval(function () {
            bridgeFetch('/poll')
            bridgePushIfChanged()              // 状态变化即上报（前后翻都覆盖）
        }, 300)
    }
}
