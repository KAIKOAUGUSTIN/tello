import assert from 'node:assert/strict';
import { test } from 'node:test';

import { distanceM, iou, labelOf, matchRect, pickAt, rectFrom } from './boxes.ts';

const det = (x, y, w, h, category = 'person') => ({ x, y, w, h, category, score: 0.8 });

test('IoU e rótulos', () => {
  assert.equal(iou({ x: 0, y: 0, w: 1, h: 1 }, { x: 0, y: 0, w: 1, h: 1 }), 1);
  assert.equal(iou({ x: 0, y: 0, w: 0.2, h: 0.2 }, { x: 0.5, y: 0.5, w: 0.2, h: 0.2 }), 0);
  assert.ok(Math.abs(iou({ x: 0, y: 0, w: 0.2, h: 0.2 }, { x: 0.1, y: 0, w: 0.2, h: 0.2 }) - 1 / 3) < 1e-9);
  assert.equal(labelOf('car'), 'Carro');
  assert.equal(labelOf('sports ball'), 'Bola');
  assert.equal(labelOf(null), 'Objeto');
});

test('área arrastada em qualquer direção, cortada nas bordas', () => {
  const near = (a, b) => Object.keys(b).every((k) => Math.abs(a[k] - b[k]) < 1e-9);
  assert.ok(near(rectFrom(0.6, 0.7, 0.2, 0.1), { x: 0.2, y: 0.1, w: 0.4, h: 0.6 }));
  assert.ok(near(rectFrom(-0.2, 0.5, 0.3, 1.4), { x: 0, y: 0.5, w: 0.3, h: 0.5 }));
});

test('toque escolhe a menor caixa sob o dedo', () => {
  const car = det(0.1, 0.4, 0.6, 0.4, 'car');
  const person = det(0.3, 0.3, 0.15, 0.5);
  assert.equal(pickAt([car, person], 0.35, 0.5), person, 'pessoa na frente do carro');
  assert.equal(pickAt([car, person], 0.6, 0.7), car);
  assert.equal(pickAt([car, person], 0.9, 0.1), null);
});

test('área marcada casa com a detecção certa ou vira objeto genérico', () => {
  const person = det(0.3, 0.3, 0.15, 0.5);
  const dog = det(0.6, 0.7, 0.1, 0.1, 'dog');
  // Caixa parecida com a da pessoa.
  assert.equal(matchRect([person, dog], { x: 0.28, y: 0.32, w: 0.17, h: 0.45 }), person);
  // Área um pouco maior em volta do cachorro.
  assert.equal(matchRect([person, dog], { x: 0.55, y: 0.65, w: 0.18, h: 0.18 }), dog);
  // Área enorme não "pega" o cachorro pequeno lá dentro.
  assert.equal(matchRect([dog], { x: 0, y: 0, w: 1, h: 1 }), null);
  // Área vazia: objeto genérico.
  assert.equal(matchRect([person, dog], { x: 0.8, y: 0.05, w: 0.1, h: 0.1 }), null);
});

test('distância aproximada pela altura da caixa', () => {
  const d = distanceM({ x: 0, y: 0, w: 0.2, h: 0.53 }, 'person');
  assert.ok(Math.abs(d - 3) < 0.1, `pessoa ocupando 53% da altura: ${d.toFixed(2)} m`);
  assert.equal(distanceM({ x: 0, y: 0, w: 0.2, h: 0.2 }, null), null);
});
