// GNOME 50 removed the X11 stage input-region option from LayoutManager.
// Actor reactivity controls native Wayland picking on every supported release.
export function chromeParameters(major, input = false) {
    const params = {affectsStruts: false, trackFullscreen: false};
    if (major < 50) params.affectsInputRegion = input;
    return params;
}
