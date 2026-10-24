//! Read-only desktop appearance. No Screenshot/RemoteDesktop requests, shell
//! commands or changes to the user's desktop preferences.
use std::collections::HashMap;
use std::sync::{atomic::{AtomicBool, Ordering}, Once};
use std::time::Duration;
use futures_lite::future;
use zbus::zvariant::OwnedValue;

type Settings = HashMap<String, HashMap<String, OwnedValue>>;
static DARK: AtomicBool = AtomicBool::new(false);
static START: Once = Once::new();
const APPEARANCE: &str = "org.freedesktop.appearance";

pub fn dark() -> bool { DARK.load(Ordering::Acquire) }

fn decode(settings: &Settings) -> bool {
    // Portal specification: 1=dark, 2=light, 0/unknown=no preference (light).
    settings.get(APPEARANCE).and_then(|s| s.get("color-scheme"))
        .and_then(|v| u32::try_from(v).ok()) == Some(1)
}

async fn read() -> zbus::Result<bool> {
    let conn = zbus::Connection::session().await?;
    read_on(&conn).await
}

async fn read_on(conn: &zbus::Connection) -> zbus::Result<bool> {
    let proxy = zbus::Proxy::new(conn, "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings").await?;
    // ReadAll also works with v1 portals and avoids Read's nested-variant bug.
    let settings: Settings = proxy.call("ReadAll", &(vec![APPEARANCE],)).await?;
    Ok(decode(&settings))
}

async fn bounded_read() -> Option<bool> {
    future::race(async { read().await.ok() }, async {
        async_io::Timer::after(Duration::from_millis(750)).await;
        None
    }).await
}

pub fn start() {
    START.call_once(|| {
        let (ready, first) = std::sync::mpsc::sync_channel(1);
        if let Err(error) = std::thread::Builder::new().name("desktop-appearance".into()).spawn(move || {
            let mut ready = Some(ready);
            loop {
                let value = future::block_on(bounded_read());
                if let Some(value) = value { DARK.store(value, Ordering::Release); }
                // A missing/restarting portal retains the last known value;
                // the first failed read uses light. Manual modes always work.
                if let Some(ready) = ready.take() { let _ = ready.send(()); }
                std::thread::sleep(Duration::from_secs(if value.is_some() { 2 } else { 10 }));
            }
        }) {
            log::warn!("系统外观读取不可用：{error}");
        } else {
            // Resolve before the first startup frame without an unbounded UI stall.
            let _ = first.recv_timeout(Duration::from_millis(900));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::sync::{Arc, atomic::AtomicU32};

    struct SettingsPortal(Arc<AtomicU32>);
    #[zbus::interface(name = "org.freedesktop.portal.Settings")]
    impl SettingsPortal {
        fn read_all(&self, namespaces: Vec<String>) -> Settings {
            assert_eq!(namespaces, [APPEARANCE]);
            HashMap::from([(APPEARANCE.into(), HashMap::from([
                ("color-scheme".into(), OwnedValue::from(self.0.load(Ordering::Acquire)))]))])
        }
    }

    #[test]
    fn private_bus_reads_live_changes_through_the_v1_compatible_settings_interface() {
        struct Bus(std::process::Child);
        impl Drop for Bus { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
        // A newly created bus has no host desktop, portal or user configuration.
        let mut bus = Bus(std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped()).spawn().unwrap());
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap()).read_line(&mut address).unwrap();
        let value = Arc::new(AtomicU32::new(0));
        future::block_on(future::race(async {
            let server = zbus::connection::Builder::address(address.trim()).unwrap()
                .name("org.freedesktop.portal.Desktop").unwrap()
                .serve_at("/org/freedesktop/portal/desktop", SettingsPortal(value.clone())).unwrap()
                .build().await.unwrap();
            let client = zbus::connection::Builder::address(address.trim()).unwrap().build().await.unwrap();
            for scheme in [0, 1, 2, 1, 99] {
                value.store(scheme, Ordering::Release);
                assert_eq!(read_on(&client).await.unwrap(), scheme == 1);
            }
            drop(server);
        }, async {
            async_io::Timer::after(Duration::from_secs(5)).await;
            panic!("private appearance bus timed out");
        }));
    }

    #[test]
    fn portal_values_follow_the_standard_and_handle_missing_or_malformed_settings() {
        for (value, expected) in [(0, false), (1, true), (2, false), (99, false)] {
            let settings = HashMap::from([(APPEARANCE.into(), HashMap::from([
                ("color-scheme".into(), OwnedValue::from(value as u32))]))]);
            assert_eq!(decode(&settings), expected);
        }
        assert!(!decode(&HashMap::new()));
        assert!(!decode(&HashMap::from([(APPEARANCE.into(), HashMap::from([
            ("color-scheme".into(), OwnedValue::from(true))]))])));
    }
}
