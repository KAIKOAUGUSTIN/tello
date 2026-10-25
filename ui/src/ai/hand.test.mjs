import assert from 'node:assert/strict';
import { test } from 'node:test';

import { GestureFilter, HAND, handCrop, PalmGuide, PhotoTrigger, pickCommander, toGesture } from './hand.ts';

const ASPECT = 4 / 3;
const box = (cx, cy, w, h) => ({ x: cx - w / 2, y: cy - h / 2, w, h });
const person = (cx, cy, w, h) => ({ ...box(cx, cy, w, h), category: 'person', score: 0.8 });

/** Alimenta o filtro a 10 Hz e devolve o gesto valendo a cada amostra. */
function feed(filter, gestures, t0 = 0) {
  return gestures.map((g, i) => filter.update(g, t0 + i * 100));
}

test('nomes do MediaPipe e confiança mínima', () => {
  assert.equal(toGesture('Victory', 0.9), 'victory');
  assert.equal(toGesture('Open_Palm', 0.7), 'palm');
  assert.equal(toGesture('Closed_Fist', 0.6), 'fist');
  assert.equal(toGesture('Thumb_Up', 0.9), 'none');
  assert.equal(toGesture('Victory', 0.3), 'none');
  assert.equal(toGesture(undefined, 1), 'none');
});

test('filtro: o gesto só vale depois de repetir e solta quando some', () => {
  const f = new GestureFilter();
  const out = feed(f, ['none', 'none', 'none', 'none', 'none', 'palm', 'palm', 'palm', 'palm', 'palm', 'palm']);
  const on = out.indexOf('palm');
  // Com a janela cheia de "nada", precisa de 70% dela: uns 0,4 s.
  assert.ok(on >= 8 && on <= 10, `ligou na amostra ${on}`);
  assert.equal(f.since, on * 100);
  // Um quadro sem a mão não solta.
  assert.deepEqual(feed(f, ['none', 'palm', 'palm'], 1100), ['palm', 'palm', 'palm']);
  // Sem a mão por mais de 0,3 s, solta.
  const off = feed(f, ['none', 'none', 'none', 'none', 'none'], 1400);
  assert.equal(off.indexOf('none'), 3, `soltou em ${off.indexOf('none')}`);
});

test('filtro: ruído não liga nada e um gesto troca pelo outro', () => {
  const f = new GestureFilter();
  assert.ok(!feed(f, ['palm', 'none', 'victory', 'none', 'palm', 'fist', 'none', 'victory']).some((g) => g !== 'none'));
  const g = new GestureFilter();
  feed(g, ['palm', 'palm', 'palm', 'palm']);
  assert.equal(g.current, 'palm');
  const out = feed(g, ['fist', 'fist', 'fist', 'fist', 'fist', 'fist'], 400);
  assert.equal(out.at(-1), 'fist');
});

test('guia: a mão sobe, o drone sobe; vai para o lado da pessoa, o drone vai junto', () => {
  const body = box(0.5, 0.55, 0.2, 0.6);
  const start = { x: 0.6, y: 0.4 };
  const guide = new PalmGuide(start, body, ASPECT);
  const step = (palm, b = body, h = 150) => guide.step(palm, b, ASPECT, h);
  assert.deepEqual(step(start), { roll: 0, throttle: 0 });
  // Um tremido pequeno fica na zona morta.
  assert.deepEqual(step({ x: 0.61, y: 0.41 }), { roll: 0, throttle: 0 });
  assert.ok(step({ x: 0.6, y: 0.3 }).throttle > 0, 'mão acima: sobe');
  assert.ok(step({ x: 0.6, y: 0.5 }).throttle < 0, 'mão abaixo: desce');
  // Mão para a direita da pessoa = esquerda da imagem = esquerda do drone (roll negativo).
  assert.ok(step({ x: 0.45, y: 0.4 }).roll < 0);
  assert.ok(step({ x: 0.75, y: 0.4 }).roll > 0);
  // Limites.
  const far = step({ x: 1, y: 0 });
  assert.equal(far.throttle, HAND.throttleMax);
  assert.equal(far.roll, HAND.rollMax);
});

test('guia: o drone se mexendo não vira comando (corpo e mão andam juntos)', () => {
  const body = box(0.5, 0.55, 0.2, 0.6);
  const guide = new PalmGuide({ x: 0.6, y: 0.4 }, body, ASPECT);
  // O drone subiu e girou: tudo desceu e foi para o lado na imagem.
  const moved = guide.step({ x: 0.45, y: 0.55 }, box(0.35, 0.7, 0.2, 0.6), ASPECT, 150);
  assert.deepEqual(moved, { roll: 0, throttle: 0 });
  // Mais longe (pessoa menor), o mesmo gesto em centímetros dá o mesmo comando.
  const near = guide.step({ x: 0.6, y: 0.4 - 0.15 * 0.6 }, body, ASPECT, 150).throttle;
  const smaller = box(0.5, 0.55, 0.1, 0.3);
  const farGuide = new PalmGuide({ x: 0.55, y: 0.475 }, smaller, ASPECT);
  const far = farGuide.step({ x: 0.55, y: 0.475 - 0.15 * 0.3 }, smaller, ASPECT, 150).throttle;
  assert.ok(Math.abs(near - far) < 1e-9, `${near} vs ${far}`);
});

test('guia: perto do chão a mão não faz descer', () => {
  const body = box(0.5, 0.55, 0.2, 0.6);
  const guide = new PalmGuide({ x: 0.6, y: 0.4 }, body, ASPECT);
  const down = { x: 0.6, y: 0.55 };
  assert.ok(guide.step(down, body, ASPECT, 120).throttle < 0);
  assert.equal(guide.step(down, body, ASPECT, 40).throttle, 0);
  assert.equal(guide.step(down, body, ASPECT, null).throttle, 0);
  assert.ok(guide.step({ x: 0.6, y: 0.25 }, body, ASPECT, 40).throttle > 0, 'subir continua valendo');
});

test('V: segura meio segundo, conta 3 s e tira a foto', () => {
  const p = new PhotoTrigger();
  assert.equal(p.see('victory', 1000, 1300), false, 'V recente ainda não vale');
  assert.equal(p.see('victory', 1000, 1500), true);
  assert.equal(p.remaining(1500), 3);
  assert.equal(p.remaining(2600), 2);
  assert.equal(p.remaining(4400), 1);
  // Baixar a mão durante a contagem não cancela.
  p.see('none', 4000, 4000);
  assert.equal(p.tick(4400), false);
  assert.equal(p.tick(4500), true);
  assert.equal(p.remaining(4600), null);
  assert.equal(p.tick(4600), false, 'uma foto só');
});

test('V: cancelar, intervalo entre fotos e o V precisa sumir e voltar', () => {
  const p = new PhotoTrigger();
  assert.ok(p.see('victory', 0, 500));
  p.cancel();
  assert.equal(p.tick(5000), false, 'cancelada (controle manual)');
  assert.equal(p.see('victory', 0, 6000), false, 'o mesmo V não recomeça');
  p.see('none', 6100, 6100);
  assert.ok(p.see('victory', 6200, 6700));
  assert.ok(p.tick(9700));
  p.see('none', 9800, 9800);
  assert.equal(p.see('victory', 10000, 10600), false, 'menos de 5 s depois da foto');
  assert.ok(p.see('victory', 10000, 14800));
});

test('recorte: quadrado em pixels, na parte de cima da pessoa e dentro do quadro', () => {
  const body = box(0.5, 0.55, 0.15, 0.6);
  const c = handCrop(body, ASPECT);
  assert.ok(Math.abs(c.w * ASPECT - c.h) < 1e-9, 'quadrado');
  assert.ok(c.y <= body.y && c.y + c.h >= body.y + body.h * 0.65, 'da cabeça até a cintura');
  assert.ok(c.y + c.h < body.y + body.h, 'sem as pernas');
  // Pessoa na borda e enorme: o recorte fica dentro do quadro.
  for (const b of [box(0.95, 0.5, 0.2, 0.9), box(0.5, 0.5, 1, 1)]) {
    const e = handCrop(b, ASPECT);
    assert.ok(e.x >= 0 && e.y >= 0 && e.x + e.w <= 1 + 1e-9 && e.y + e.h <= 1 + 1e-9, JSON.stringify(e));
  }
  // Pessoa pequena (longe): um mínimo, para a mão não sumir no recorte.
  assert.ok(handCrop(box(0.5, 0.5, 0.02, 0.06), ASPECT).h >= HAND.cropMin);
});

test('quem comanda: a mesma pessoa enquanto der; senão, a maior perto do centro', () => {
  const center = person(0.5, 0.5, 0.15, 0.5);
  const side = person(0.85, 0.5, 0.2, 0.6);
  const car = { ...box(0.5, 0.7, 0.4, 0.3), category: 'car', score: 0.9 };
  assert.equal(pickCommander([side, center, car], null), center);
  assert.equal(pickCommander([side, center], side), side, 'continua com quem já comandava');
  assert.equal(pickCommander([car], null), null);
});
