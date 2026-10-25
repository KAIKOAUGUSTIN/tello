import assert from 'node:assert/strict';
import { test } from 'node:test';

import { AI_STALE_MS, pick } from './mix.ts';

const still = { roll: 0, pitch: 0, throttle: 0, yaw: 0 };
const moving = { roll: 0, pitch: 0.3, throttle: 0, yaw: 0 };

test('a fonte manual mais recente comanda', () => {
  const sources = new Map([
    ['gamepad', { sticks: moving, activeAt: 10, at: 50 }],
    ['keyboard', { sticks: moving, activeAt: 20, at: 50 }],
    ['mouse', { sticks: still, activeAt: 40, at: 50 }],
  ]);
  assert.equal(pick(sources, 50), 'keyboard');
});

test('qualquer fonte manual fora do centro ganha da IA', () => {
  const sources = new Map([
    ['ai', { sticks: moving, activeAt: 100, at: 100 }],
    ['mouse', { sticks: moving, activeAt: 1, at: 1 }],
  ]);
  assert.equal(pick(sources, 100), 'mouse');
});

test('a IA só comanda com relatos recentes', () => {
  const sources = new Map([
    ['ai', { sticks: moving, activeAt: 0, at: 0 }],
    ['mouse', { sticks: still, activeAt: 0, at: 0 }],
  ]);
  assert.equal(pick(sources, AI_STALE_MS), 'ai');
  assert.equal(pick(sources, AI_STALE_MS + 1), null);
  sources.set('ai', { sticks: still, activeAt: 0, at: 0 });
  assert.equal(pick(sources, 0), null);
});
