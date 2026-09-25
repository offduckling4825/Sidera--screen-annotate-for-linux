// ============================================================
// Sidera - xdg-desktop-portal 截图（KWin/GNOME 等无 wlr-screencopy 的合成器）
// 非阻塞：在新线程里调用门户，成功后存 PNG 并复制到剪贴板
// ============================================================
use std::collections::HashMap;

use zbus::zvariant::Value;

/// 非阻塞提交一次门户截图
pub fn screenshot_nonblocking() {
    std::thread::spawn(|| {
        if let Err(e) = run() {
            log::warn!("[WARN] 门户截图失败: {}", e);
        }
    });
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let conn = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )?;

    let mut opts = HashMap::new();
    opts.insert("interactive".to_string(), Value::Bool(false));
    opts.insert("modal".to_string(), Value::Bool(false));

    let handle: zbus::zvariant::OwnedObjectPath = proxy.call("Screenshot", &("", opts))?;

    let req = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        &handle,
        "org.freedesktop.portal.Request",
    )?;

    let mut found_uri: Option<String> = None;
    for msg in req.receive_signal("Response")? {
        let body = msg.body();
        let (code, results): (u32, HashMap<String, Value>) = body.deserialize()?;
        if code == 0 {
            if let Some(Value::Str(uri)) = results.get("uri") {
                found_uri = Some(uri.to_string());
            }
        }
        break;
    }

    let Some(uri) = found_uri else {
        return Err("门户未返回截图 URI（可能被取消或权限不足）".into());
    };
    let path = uri
        .strip_prefix("file://")
        .unwrap_or(uri.as_str())
        .to_string();
    let bytes = std::fs::read(&path)?;
    let img = image::load_from_memory(&bytes)?.to_rgba8();
    let (w, h) = img.dimensions();
    crate::save_and_clipboard(img.into_raw(), w, h);
    Ok(())
}
