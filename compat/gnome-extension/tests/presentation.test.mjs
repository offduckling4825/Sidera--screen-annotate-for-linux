import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../presentation.js', import.meta.url), 'utf8');
const {isWpsPresentation} = await import(`data:text/javascript,${encodeURIComponent(source)}`);
const cases = JSON.parse(readFileSync(new URL('./wps-presentation-cases.json', import.meta.url), 'utf8'));

test('shared WPS presentation vectors match the X11 classifier', () => {
    for (const {instance, class: windowClass, title, expected} of cases)
        assert.equal(isWpsPresentation(instance, windowClass, title), expected, `${instance}/${windowClass}: ${title}`);
});
