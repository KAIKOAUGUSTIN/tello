import assert from 'node:assert/strict';
import { test } from 'node:test';

import { JOINTS, LM, point, POINTS } from './pose.ts';
import { figure } from './synthetic.ts';

test('a pose sintética fica de frente para a câmera', () => {
  const p = figure({ cx: 0.5, height: 0.6 });
  assert.equal(p.length, POINTS * 3);
  // O lado direito da pessoa aparece à esquerda da imagem.
  assert.ok(point(p, LM.rightShoulder).x < point(p, LM.leftShoulder).x);
  assert.ok(point(p, LM.rightHip).x < point(p, LM.leftHip).x);
  // De cima para baixo: nariz, ombros, quadris, tornozelos.
  assert.ok(point(p, LM.nose).y < point(p, LM.leftShoulder).y);
  assert.ok(point(p, LM.leftShoulder).y < point(p, LM.leftHip).y);
  assert.ok(point(p, LM.leftHip).y < point(p, LM.leftAnkle).y);
  for (const j of JOINTS) {
    const { x, y, v } = point(p, j);
    assert.ok(x > 0 && x < 1 && y > 0 && y < 1, `ponto ${j} fora da imagem`);
    assert.ok(v > 0.9);
  }
});

test('braços da pose sintética', () => {
  const up = figure({ cx: 0.5, height: 0.6, left: 'up', right: 'up' });
  assert.ok(point(up, LM.leftWrist).y < point(up, LM.nose).y);
  assert.ok(point(up, LM.rightWrist).y < point(up, LM.nose).y);

  const side = figure({ cx: 0.5, height: 0.6, right: 'side' });
  const shoulder = point(side, LM.rightShoulder);
  const wrist = point(side, LM.rightWrist);
  assert.ok(Math.abs(wrist.y - shoulder.y) < 1e-6);
  assert.ok(wrist.x < shoulder.x, 'o braço direito aponta para a esquerda da imagem');

  const down = figure({ cx: 0.5, height: 0.6, left: 'down' });
  assert.ok(point(down, LM.leftWrist).y > point(down, LM.leftShoulder).y);
  assert.ok(point(down, LM.leftWrist).x > point(down, LM.leftShoulder).x);
});
