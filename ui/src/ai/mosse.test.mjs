import assert from 'node:assert/strict';
import test from 'node:test';

import { MOSSE, Mosse } from './mosse.ts';

const W = 320;
const H = 240;

// Fundo com textura suave e um alvo com desenho próprio, que pode andar e crescer.
function scene({ cx, cy, w, h, hidden = false, seed = 1 }) {
  const data = new Float32Array(W * H);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      let v = 110 + 40 * Math.sin(x * 0.045 + seed) * Math.cos(y * 0.06) + 25 * Math.sin((x + y) * 0.13);
      const u = (x - cx) / w;
      const t = (y - cy) / h;
      if (!hidden && Math.abs(u) <= 0.5 && Math.abs(t) <= 0.5) {
        v = 60 + 120 * ((Math.floor((u + 0.5) * 4) + Math.floor((t + 0.5) * 6)) % 2) + 50 * Math.sin(u * 9);
      }
      data[y * W + x] = Math.max(0, Math.min(255, v));
    }
  }
  return { data, width: W, height: H };
}

test('segue um alvo andando', () => {
  let target = { cx: 120, cy: 110, w: 40, h: 70 };
  const tracker = new Mosse(scene(target), target);
  let worst = 0;
  for (let k = 0; k < 40; k++) {
    target = { ...target, cx: target.cx + 2.5, cy: target.cy + Math.sin(k / 5) * 1.5 };
    const r = tracker.update(scene(target));
    assert.ok(r.psr >= MOSSE.lostPsr, `quadro ${k}: psr ${r.psr.toFixed(1)}`);
    worst = Math.max(worst, Math.hypot(r.box.cx - target.cx, r.box.cy - target.cy));
  }
  assert.ok(worst < 3, `erro máximo ${worst.toFixed(2)} px`);
});

test('acompanha o alvo crescendo (chegando perto)', () => {
  let target = { cx: 160, cy: 120, w: 36, h: 60 };
  const tracker = new Mosse(scene(target), target);
  let r;
  for (let k = 0; k < 15; k++) {
    target = { ...target, w: target.w * 1.025, h: target.h * 1.025 };
    r = tracker.update(scene(target));
  }
  const ratio = r.box.h / target.h;
  assert.ok(ratio > 0.88 && ratio < 1.12, `altura rastreada / real = ${ratio.toFixed(2)}`);
});

test('quando o alvo some, a confiança cai e a caixa fica parada', () => {
  const target = { cx: 160, cy: 120, w: 40, h: 60 };
  const tracker = new Mosse(scene(target), target);
  tracker.update(scene(target));
  const r = tracker.update(scene({ ...target, hidden: true }));
  assert.ok(r.psr < MOSSE.lostPsr, `psr ${r.psr.toFixed(1)}`);
  assert.ok(Math.hypot(r.box.cx - target.cx, r.box.cy - target.cy) < 1);
  // E reencontra quando ele volta.
  const back = tracker.update(scene(target));
  assert.ok(back.psr >= MOSSE.lostPsr, `psr na volta ${back.psr.toFixed(1)}`);
});

test('é rápido o bastante para todo quadro', () => {
  const target = { cx: 160, cy: 120, w: 40, h: 60 };
  const img = scene(target);
  const tracker = new Mosse(img, target);
  const started = performance.now();
  for (let k = 0; k < 30; k++) tracker.update(img);
  const ms = (performance.now() - started) / 30;
  assert.ok(ms < 15, `${ms.toFixed(1)} ms por quadro`);
});
