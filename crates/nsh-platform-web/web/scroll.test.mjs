import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

// Exercise the production player without starting its WebAssembly engine.
const source = await readFile(new URL('./nsh.js', import.meta.url), 'utf8');
const { Player } = await import('data:text/javascript;base64,' + Buffer.from(source + '\nexport { Player };').toString('base64'));
const player = Object.create(Player.prototype);
player.idiom = "desktop";
const shown = [];
player.showBar = (el) => shown.push(el);
function scroll(horizontal = false, left = 40, top = 30) {
  return {
    _kind: 'scroll', _indicator: true, _horizontal: horizontal,
    clientWidth: 300, clientHeight: 200, scrollWidth: horizontal ? 1200 : 300,
    scrollHeight: horizontal ? 200 : 800, scrollLeft: 0, scrollTop: 0,
    getBoundingClientRect: () => ({ left, top }),
  };
}

test('content hover leaves visibility and fade timer alone', () => {
  shown.length = 0;
  const el = scroll();
  el._barTimer = 123;
  player.hoverBar(el, { clientX: 150, clientY: 100 });
  assert.equal(shown.length, 0);
  assert.equal(el._barTimer, 123);
  player.hoverBar(el, { clientX: 338, clientY: 35 });
  assert.deepEqual(shown, [el]);
  shown.length = 0;
  player.hoverBar(el, { clientX: 338, clientY: 180 });
  assert.equal(shown.length, 0);
});

test('horizontal and nested scroll views use their own coordinates', () => {
  shown.length = 0;
  const outer = scroll(false, 0, 0);
  const inner = scroll(true, 40, 30);
  const e = { clientX: 45, clientY: 228 };
  for (const el of [inner, outer]) player.hoverBar(el, e);
  assert.deepEqual(shown, [inner]);
});

test('disabled, fitting and phone scroll views do not reveal an overlay', () => {
  shown.length = 0;
  const el = scroll();
  const e = { clientX: 338, clientY: 35 };
  el._behavior = { enabled: false };
  player.hoverBar(el, e);
  el._behavior.enabled = true;
  el.scrollHeight = el.clientHeight;
  player.hoverBar(el, e);
  el.scrollHeight = 800;
  player.idiom = "phone";
  player.hoverBar(el, e);
  player.idiom = "desktop";
  assert.equal(shown.length, 0);
});
