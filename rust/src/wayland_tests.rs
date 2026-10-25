use super::*;

#[test]
fn scaling_uses_only_available_protocols() {
    assert_eq!(render_scale(1.25, true, 6), 1.25);
    assert_eq!(render_scale(1.75, false, 4), 2.0);
    assert_eq!(render_scale(2.0, false, 2), 1.0);
    assert_eq!(render_scale(f64::NAN, true, 6), 1.0);
}

#[test]
fn shm_dimensions_reject_protocol_overflow() {
    assert_eq!(shm_layout(1920, 1080), Some((7680, 8_294_400)));
    for (w, h) in [(0, 10), (10, -1), (i32::MAX, 2), (32768, 32768)] {
        assert!(shm_layout(w, h).is_none(), "{w} x {h}");
    }
}

fn pool() -> (PreparedWayland, ShmFrames) {
    assert_eq!(std::env::var("SIDERA_TEST_ISOLATED").as_deref(), Ok("1"),
               "Run through tools/test_rust.py --wayland in its private compositor");
    let prepared = prepare().unwrap();
    let frames = ShmFrames::new(&prepared.shm);
    (prepared, frames)
}

fn pixels(frames: &mut ShmFrames, slot: usize) -> Vec<u8> {
    frames.slots[slot].as_mut().unwrap().pool.mmap().to_vec()
}

#[test]
#[ignore = "requires an isolated layer-shell Wayland compositor"]
fn busy_storage_is_untouched_and_release_catches_up() {
    let (prepared, mut frames) = pool();
    let qh = prepared.event_queue.handle();
    let mut canvas = Pixmap::new(4, 3).unwrap();
    canvas.data_mut()[0..4].copy_from_slice(&[255, 0, 0, 255]);
    let first = frames.write_frame(&canvas, &qh).unwrap().unwrap().clone();
    let saved_first = pixels(&mut frames, 0);
    canvas.data_mut()[4..8].copy_from_slice(&[0, 255, 0, 255]);
    let second = frames.write_frame(&canvas, &qh).unwrap().unwrap().clone();
    let saved_second = pixels(&mut frames, 1);
    assert_eq!(pixels(&mut frames, 0), saved_first);
    canvas.data_mut()[8..12].copy_from_slice(&[0, 0, 255, 255]);
    assert!(frames.write_frame(&canvas, &qh).unwrap().is_none());
    assert_eq!(pixels(&mut frames, 0), saved_first);
    assert_eq!(pixels(&mut frames, 1), saved_second);
    // Simulate delayed compositor acknowledgements. These buffers are never
    // attached by this storage test, so no live compositor owns their bytes.
    frames.release(&first);
    assert_eq!(frames.write_frame(&canvas, &qh).unwrap(), Some(&first));
    assert_eq!(&pixels(&mut frames, 0)[..12],
               &[0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255]);
    assert_eq!(pixels(&mut frames, 1), saved_second);
    frames.release(&second);
    frames.write_frame(&canvas, &qh).unwrap().unwrap();
    assert_eq!(pixels(&mut frames, 0), pixels(&mut frames, 1));
}

#[test]
#[ignore = "requires an isolated layer-shell Wayland compositor"]
fn resize_keeps_busy_pools_alive_until_their_release() {
    let (prepared, mut frames) = pool();
    let qh = prepared.event_queue.handle();
    let mut small = Pixmap::new(3, 2).unwrap();
    small.fill(tiny_skia::Color::from_rgba8(20, 40, 60, 255));
    let first = frames.write_frame(&small, &qh).unwrap().unwrap().clone();
    let second = frames.write_frame(&small, &qh).unwrap().unwrap().clone();
    let old = pixels(&mut frames, 1);
    let mut large = Pixmap::new(8, 6).unwrap();
    large.fill(tiny_skia::Color::from_rgba8(90, 100, 110, 255));
    assert!(frames.write_frame(&large, &qh).unwrap().is_none());
    assert_eq!(pixels(&mut frames, 0), old);
    frames.release(&first);
    let replacement = frames.write_frame(&large, &qh).unwrap().unwrap().clone();
    assert_ne!(replacement, first);
    assert_eq!(frames.slots[0].as_ref().unwrap().dims, (8, 6));
    assert_eq!(pixels(&mut frames, 1), old);
    assert_eq!(frames.slots[1].as_ref().unwrap().dims, (3, 2));
    frames.release(&first); // A stale identity cannot release a newer generation.
    assert!(frames.slots[0].as_ref().unwrap().busy);
    frames.release(&second);
    frames.write_frame(&large, &qh).unwrap().unwrap();
    assert_eq!(pixels(&mut frames, 0), pixels(&mut frames, 1));
    assert_eq!(frames.slots.iter().flatten().count(), 2);
}
