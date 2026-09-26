// ============================================================
// Sidera联动桥（加载项内运行）
//   · 监听 WPS 放映事件 → 上报批注 app (127.0.0.1:16666 /push)
//   · 轮询 /poll 取 NEXT/PREV 指令（保留，默认 app 用虚拟键推进）
// 配合 app 的“WPS 接口调试模式”使用：批注缓存按真实页号驱动，
// 页内动画步不动批注。
// ============================================================
var BRIDGE_URL = 'http://127.0.0.1:16666';

function OnAddinLoad(ribbonUI) {
    if (ribbonUI && typeof window.Application.ribbonUI != 'object')
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
function bridgeFetch(path) {
    try {
        fetch(BRIDGE_URL + path)
            .then(function (r) { return r.text() })
            .then(function (t) {
                var c = (t || '').trim()
                if (c) bridgeCmd(c)
            })
            .catch(function () {})
    } catch (e) {}
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
var _lastState = { pos: -1, click: -1 };
function bridgePushIfChanged() {
    try {
        var st = bridgeState()
        if (st.pos < 1) return                       // 不在放映
        if (st.pos !== _lastState.pos || st.click !== _lastState.click) {
            _lastState.pos = st.pos
            _lastState.click = st.click
            bridgeFetch('/push?m=' + encodeURIComponent('EVENT SlideShowState pos=' + st.pos + ' click=' + st.click))
        }
    } catch (e) {}
}
function startBridge() {
    // 启动探针：只要本 JS 被执行，就先报到一次，便于诊断
    try {
        if (!window._sideraBootPinged) {
            window._sideraBootPinged = true
            bridgeFetch('/hello?m=sidera-boot')
        }
    } catch (e) {}
    if (typeof window.Application == 'undefined' || !window.Application) {
        setTimeout(startBridge, 500)          // 等 Application 就绪
        return
    }
    // 事件监听为“可选增强”：即使没有 ApiEvent，也要启动轮询上报真实页号
    try {
        if (window.Application.ApiEvent) {
            for (var i = 0; i < BridgeSlideEvents.length; i++) {
                (function (ev) {
                    window.Application.ApiEvent.AddApiEventListener(ev, function () {
                        var st = bridgeState()
                        bridgeFetch('/push?m=' + encodeURIComponent('EVENT ' + ev + ' pos=' + st.pos + ' click=' + st.click))
                    })
                })(BridgeSlideEvents[i])
            }
        } else {
            setTimeout(startBridge, 500)      // 稍后再尝试注册事件，但不阻塞下面的轮询
        }
    } catch (e) {}
    bridgeFetch('/hello?m=sidera-bridge')
    // 轮询 /poll 取 NEXT/PREV；并每 300ms 上报状态变化（点屏幕翻页靠这个覆盖）
    if (!window._sideraTimerStarted) {
        window._sideraTimerStarted = true
        setInterval(function () {
            bridgeFetch('/poll')
            bridgePushIfChanged()              // 状态变化即上报（前后翻都覆盖）
        }, 300)
    }
}
