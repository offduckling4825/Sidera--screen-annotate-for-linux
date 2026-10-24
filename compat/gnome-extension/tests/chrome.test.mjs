import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../chrome.js', import.meta.url), 'utf8');
const {chromeParameters} = await import(`data:text/javascript,${encodeURIComponent(source)}`);

test('GNOME 50 accepts both visual and reactive actors without the removed parameter', () => {
    // Keys from the installed GNOME 50.1 layout.js defaultParams. Params.parse
    // rejects extra keys after attaching the actor, preventing the startup hello.
    const allowed = new Set(['affectsStruts', 'trackFullscreen']);
    for (const input of [false, true]) {
        const parameters = chromeParameters(50, input);
        for (const key of Object.keys(parameters)) assert.ok(allowed.has(key), key);
        assert.equal(parameters.trackFullscreen, false);
        assert.equal(parameters.affectsStruts, false);
    }
});

test('GNOME 46 through 49 retain their visual/input region distinction', () => {
    for (let major = 46; major < 50; major++) {
        assert.equal(chromeParameters(major, false).affectsInputRegion, false);
        assert.equal(chromeParameters(major, true).affectsInputRegion, true);
    }
});
