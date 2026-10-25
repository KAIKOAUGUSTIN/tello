import assert from 'node:assert/strict';
import { test } from 'node:test';

import { CameraMotion, moveBox, moveWith } from './motion.ts';

const W = 480;
const H = 360;

// Paisagem com detalhes de vários tamanhos e sem repetição (como uma cena de
// verdade), vista por uma câmera pinhole girada: tudo aparece `pan` rad à
// direita e `tilt` rad abaixo. `blur`: borrão do giro durante a exposição (rad).
const GROUND = 1024;
const grain = noise(3, GROUND, GROUND).data;
const TAN_H = Math.tan((35 * Math.PI) / 180);
const TAN_V = Math.tan((28 * Math.PI) / 180);
const PX_PER_RAD = 400;

function ground(x, y) {
  let v = 0;
  for (const [size, weight] of [
    [48, 0.4],
    [16, 0.35],
    [5, 0.25],
  ]) {
    const gx = (x + 600) / size;
    const gy = (y + 600) / size;
    const x0 = Math.floor(gx);
    const y0 = Math.floor(gy);
    const ax = gx - x0;
    const ay = gy - y0;
    const at = (i, j) => grain[((j * 131 + size) % GROUND) * GROUND + ((i * 17 + size * 7) % GROUND)];
    const top = at(x0, y0) + ax * (at(x0 + 1, y0) - at(x0, y0));
    const bottom = at(x0, y0 + 1) + ax * (at(x0 + 1, y0 + 1) - at(x0, y0 + 1));
    v += weight * (top + ay * (bottom - top));
  }
  return 40 + 0.7 * v;
}

function view(pan, tilt, blur = 0) {
  const taps = blur ? 9 : 1;
  const data = new Float32Array(W * H);
  for (let y = 0; y < H; y++) {
    const b = (Math.atan(((y + 0.5) / H - 0.5) * 2 * TAN_V) - tilt) * PX_PER_RAD;
    for (let x = 0; x < W; x++) {
      const a = Math.atan(((x + 0.5) / W - 0.5) * 2 * TAN_H) - pan;
      let v = 0;
      for (let k = 0; k < taps; k++) v += ground((a + (blur * k) / taps) * PX_PER_RAD, b);
      data[y * W + x] = v / taps;
    }
  }
  return { data, width: W, height: H };
}

function noise(seed, width = W, height = H) {
  let s = seed;
  const data = new Float32Array(width * height);
  for (let i = 0; i < data.length; i++) data[i] = ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 2 ** 32) * 255;
  return { data, width, height };
}

const deg = (r) => (r * 180) / Math.PI;

test('mede o giro da câmera entre dois quadros, mesmo borrado', () => {
  for (const [panDeg, tiltDeg, blurDeg] of [
    [0, 0, 0],
    [2, 0, 0],
    [-6, 0, 0],
    [-11, 0.8, 0],
    [4, -2.5, 0],
    [-7, 0, 2],
    [-14, 0, 0],
  ]) {
    const [pan, tilt, blur] = [panDeg, tiltDeg, blurDeg].map((d) => (d * Math.PI) / 180);
    const camera = new CameraMotion();
    assert.equal(camera.update(view(0, 0, blur), 0), null, 'o primeiro quadro só serve de referência');
    const m = camera.update(view(pan, tilt, blur), 66);
    assert.ok(m, `sem medida para ${panDeg}°, ${tiltDeg}°`);
    const ok = Math.abs(deg(m.pan - pan)) < 0.2 && Math.abs(deg(m.tilt - tilt)) < 0.2;
    assert.ok(ok, `${panDeg}°, ${tiltDeg}°: mediu ${deg(m.pan).toFixed(2)}°, ${deg(m.tilt).toFixed(2)}°`);
  }
});

test('cena trocada não vira giro', () => {
  const camera = new CameraMotion();
  camera.update(noise(1), 0);
  assert.equal(camera.update(noise(2), 66), null);
  // Depois de esquecer, o próximo quadro volta a ser só referência.
  camera.reset();
  assert.equal(camera.update(view(0, 0), 133), null);
});

test('quadro sem medida no meio do giro: o giro de há pouco continua por um instante', () => {
  const step = (-5 * Math.PI) / 180;
  const camera = new CameraMotion();
  camera.update(view(0, 0), 0);
  assert.ok(Math.abs(camera.update(view(step, 0), 100).pan - step) < 0.004);
  // Um quadro que não casa com o anterior (aqui, ruído): mesmo giro por segundo, no intervalo dele.
  const held = camera.update(noise(5), 150);
  assert.ok(held && Math.abs(held.pan - step / 2) < 0.004, `continuou com ${held && deg(held.pan).toFixed(2)}°`);
  // Passado o tempo, sem medida é sem medida.
  camera.update(noise(6), 300);
  assert.equal(camera.update(noise(7), 500), null);
});

test('câmera pinhole: na borda a imagem anda mais que no centro, e girar de volta desfaz', () => {
  const m = { pan: -0.1, tilt: 0.05 };
  const center = moveWith(m, 0.5, 0.5);
  const edge = moveWith(m, 0.9, 0.5);
  assert.ok(center.x < 0.5 && center.y > 0.5, 'pan negativo leva para a esquerda; tilt positivo, para baixo');
  assert.ok(0.9 - edge.x > 0.5 - center.x, 'na borda anda mais');
  const back = moveWith({ pan: 0.1, tilt: -0.05 }, edge.x, edge.y);
  assert.ok(Math.abs(back.x - 0.9) < 1e-9 && Math.abs(back.y - 0.5) < 1e-9);
  const b = moveBox({ x: 0.7, y: 0.4, w: 0.1, h: 0.2 }, m);
  assert.equal(b.w, 0.1);
  assert.equal(b.h, 0.2);
  assert.ok(Math.abs(b.x + b.w / 2 - moveWith(m, 0.75, 0.5).x) < 1e-9);
});
