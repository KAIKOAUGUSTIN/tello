import assert from 'node:assert/strict';
import { test } from 'node:test';

import { FOLLOW, FollowController, slew } from './follow.ts';
import { sizeOf, Tracker } from './tracker.ts';

const box = (cx, cy, w, h) => ({ x: cx - w / 2, y: cy - h / 2, w, h });
const det = (cx, cy, w, h, category = 'person', score = 0.8) => ({ ...box(cx, cy, w, h), category, score });
const still = (cx, size) => ({ cx, cy: 0.5, size, vx: 0, vy: 0, vSize: 0 });

test('rastreador: segue o rastreador visual e estima a velocidade', () => {
  const tracker = new Tracker(box(0.3, 0.5, 0.1, 0.3), 'person', 0);
  let cx = 0.3;
  for (let t = 33; t <= 1000; t += 33) {
    cx += 0.2 * 0.033;
    assert.ok(tracker.update({ box: box(cx, 0.5, 0.1, 0.3), psr: 20 }, null, t));
  }
  const est = tracker.estimate();
  assert.ok(Math.abs(est.cx - cx) < 0.01, `posição ${est.cx.toFixed(3)} vs ${cx.toFixed(3)}`);
  assert.ok(Math.abs(est.vx - 0.2) < 0.03, `velocidade ${est.vx.toFixed(3)}`);
  assert.ok(Math.abs(tracker.estimate(0.5).cx - (cx + 0.1)) < 0.02, 'prevê meio segundo à frente');
});

test('rastreador: a detecção corrige a deriva e pede para recomeçar o visual', () => {
  const tracker = new Tracker(box(0.5, 0.5, 0.1, 0.3), 'person', 0);
  // O visual derivou um pouco; a detecção (mesma classe) mostra a caixa certa.
  const right = det(0.53, 0.5, 0.12, 0.34);
  const seen = tracker.update({ box: box(0.5, 0.5, 0.1, 0.3), psr: 15 }, [det(0.2, 0.5, 0.1, 0.3, 'car'), right], 33);
  assert.deepEqual(seen, right);
  assert.deepEqual(tracker.reinit, right);
  // Detecção quase igual ao visual: usa, mas não recomeça.
  tracker.update({ box: box(0.53, 0.5, 0.12, 0.34), psr: 15 }, [det(0.535, 0.5, 0.12, 0.34)], 66);
  assert.equal(tracker.reinit, null);
  // Mesmo centro, mas o visual encolheu 12%: o IoU ainda é alto, só que a distância sairia errada.
  const full = det(0.53, 0.5, 0.12, 0.34);
  tracker.update({ box: box(0.53, 0.5, 0.106, 0.3), psr: 15 }, [full], 99);
  assert.deepEqual(tracker.reinit, full);
});

test('rastreador: perdido conta o tempo e reencontra só perto e do mesmo tamanho', () => {
  const tracker = new Tracker(box(0.5, 0.5, 0.1, 0.3), 'person', 0);
  assert.equal(tracker.update({ box: box(0.5, 0.5, 0.1, 0.3), psr: 3 }, null, 100), null);
  assert.equal(tracker.lostFor(1100), 1000);
  // Outra pessoa longe, ou do tamanho errado no lugar certo: não serve.
  assert.equal(tracker.update(null, [det(0.95, 0.5, 0.1, 0.3), det(0.5, 0.5, 0.05, 0.1)], 1200), null);
  // Outra classe no lugar certo também não.
  assert.equal(tracker.update(null, [det(0.52, 0.5, 0.1, 0.3, 'dog')], 1300), null);
  const back = det(0.55, 0.5, 0.1, 0.31);
  assert.deepEqual(tracker.update(null, [back], 1400), back);
  assert.deepEqual(tracker.reinit, back);
  assert.equal(tracker.lostFor(1400), 0);
});

test('rastreador: objeto genérico depende só do visual e usa a área como tamanho', () => {
  const tracker = new Tracker(box(0.5, 0.5, 0.2, 0.1), null, 0);
  assert.equal(tracker.label, 'Objeto');
  assert.ok(Math.abs(tracker.estimate().size - Math.sqrt(0.02)) < 1e-6);
  assert.equal(tracker.update(null, [det(0.5, 0.5, 0.2, 0.1)], 33), null, 'detecção não reencontra genérico');
  assert.equal(sizeOf(box(0, 0, 0.2, 0.5), false), 0.5);
});

test('controle: zona morta, sentido, limites e modo rápido', () => {
  const c = new FollowController(0.4);
  assert.deepEqual(c.step(still(0.52, 0.4)), { yaw: 0, pitch: 0 });
  assert.equal(c.step(still(0.95, 0.4)).yaw, FOLLOW.yawMax, 'alvo à direita: gira para a direita, no limite');
  const far = c.step(still(0.5, 0.25));
  assert.ok(far.pitch > 0 && far.pitch <= FOLLOW.forwardMax, 'longe: avança');
  assert.equal(c.step(still(0.5, 0.25), { bottom: 0.99 }).pitch, 0, 'cortado embaixo: não avança');
  assert.equal(c.step(still(0.5, 0.7)).pitch, -FOLLOW.backMax, 'perto demais: recua no máximo');
  const fast = c.step(still(0.95, 0.7), { fast: true });
  assert.equal(fast.yaw, FOLLOW.yawMax / 2);
  assert.equal(fast.pitch, -FOLLOW.backMax / 2);
  assert.deepEqual(c.step(null), { yaw: 0, pitch: 0 });
  // Alvo indo para a direita: começa a girar antes de sair da zona morta.
  assert.ok(new FollowController(0.4).step({ ...still(0.5, 0.4), vx: 0.3 }).yaw > 0);
});

test('limite de variação', () => {
  assert.deepEqual(slew({ yaw: 0, pitch: 0 }, { yaw: 0.35, pitch: -0.05 }, 0.1), { yaw: 0.1, pitch: -0.05 });
});

/** Normal com semente fixa (o teste sai sempre igual). */
function noiseSource(seed = 7) {
  let s = seed;
  const rand = () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 2 ** 32);
  return () => Math.sqrt(-2 * Math.log(rand() + 1e-12)) * Math.cos(2 * Math.PI * rand());
}

/**
 * Simulação em malha fechada: o drone e uma pessoa (1,7 m), com o atraso do
 * vídeo, a taxa de análises, ruído na caixa e a resposta do drone.
 */
function simulate({ hz, yawRate = 100, speed = 2, bearing0 = 25, dist0 = 3, targetDist = 3, fast = false, walk = 0 }) {
  const HFOV = 70;
  const LATENCY = 0.25;
  const DT = 0.005;
  const height = (d) => 1.6 / d;
  const noise = noiseSource();
  const measure = (b, d) => box(0.5 + b / HFOV + 0.003 * noise(), 0.5, 0.4 * height(d), height(d) * (1 + 0.01 * noise()));
  const tracker = new Tracker(measure(bearing0, dist0), 'person', 0);
  const controller = new FollowController(height(targetDist));
  let bearing = bearing0; // graus: pessoa à direita do nariz do drone
  let dist = dist0;
  let yawVel = 0;
  let fwdVel = 0;
  let out = { yaw: 0, pitch: 0 };
  let cmd = { yaw: 0, pitch: 0 };
  const history = [];
  const trace = [];
  let nextAnalysis = 0;
  for (let t = 0; t < 12; t += DT) {
    history.push({ t, bearing, dist });
    if (t >= nextAnalysis) {
      nextAnalysis += 1 / hz;
      const seen = history.find((h) => h.t >= t - LATENCY) ?? history[0];
      const b = measure(seen.bearing, seen.dist);
      tracker.update({ box: b, psr: 20 }, null, t * 1000);
      cmd = controller.step(tracker.estimate(), { bottom: b.y + b.h, dt: 1 / hz, fast });
    }
    out = slew(out, cmd, FOLLOW.slewPerS * DT);
    // Resposta de primeira ordem do drone (giro rápido, translação mais lenta).
    yawVel += ((out.yaw * yawRate - yawVel) * DT) / 0.15;
    fwdVel += ((out.pitch * speed - fwdVel) * DT) / 0.4;
    bearing += ((walk / dist) * 180) / Math.PI * DT - yawVel * DT;
    dist -= fwdVel * DT;
    trace.push({ t, cx: 0.5 + bearing / HFOV, dist });
  }
  return trace;
}

function settleTime(trace, ok) {
  let last = 0;
  for (const p of trace) if (!ok(p)) last = p.t;
  return last;
}

const sizeErr = (dist, targetDist) => (1.6 / targetDist - 1.6 / dist) / (1.6 / targetDist);

for (const hz of [10, 15, 30]) {
  for (const yawRate of [60, 100]) {
    test(`giro: centraliza rápido e sem oscilar (${hz} análises/s, ${yawRate}°/s)`, () => {
      const trace = simulate({ hz, yawRate });
      const band = FOLLOW.yawDeadband + 0.03;
      const settle = settleTime(trace, (p) => Math.abs(p.cx - 0.5) <= band);
      assert.ok(settle < 2, `centraliza em ${settle.toFixed(2)} s`);
      const overshoot = Math.max(...trace.map((p) => 0.5 - p.cx));
      assert.ok(overshoot <= 0.1, `ultrapassa ${overshoot.toFixed(3)}`);
    });

    test(`giro: acompanha alguém andando de lado a 1,5 m/s (${hz} análises/s, ${yawRate}°/s)`, () => {
      const trace = simulate({ hz, yawRate, bearing0: 0, walk: 1.5 });
      const offset = (from) => Math.max(...trace.filter((p) => p.t > from).map((p) => Math.abs(p.cx - 0.5)));
      assert.ok(offset(2) < 0.15, `fica a até ${offset(2).toFixed(3)} do centro`);
      assert.ok(offset(6) < 0.08, `depois de 6 s, a até ${offset(6).toFixed(3)}`);
    });
  }

  // Velocidade com o stick no máximo: modo normal (2 a 4 m/s) e modo rápido (8 m/s, limites pela metade).
  for (const [speed, fast] of [
    [2, false],
    [4, false],
    [8, true],
  ]) {
    test(`distância: chega sem passar do ponto (${hz} análises/s, ${speed} m/s${fast ? ', modo rápido' : ''})`, () => {
      for (const [dist0, targetDist] of [
        [4.5, 3],
        [2, 3],
      ]) {
        const trace = simulate({ hz, speed, fast, bearing0: 0, dist0, targetDist });
        const ok = (p) => Math.abs(sizeErr(p.dist, targetDist)) <= FOLLOW.pitchDeadband + 0.04;
        const settle = settleTime(trace, ok);
        assert.ok(settle < 5, `chega em ${settle.toFixed(2)} s (de ${dist0} m)`);
        const crossed = trace.filter((p) => Math.sign(p.dist - targetDist) !== Math.sign(dist0 - targetDist));
        const worst = Math.max(0, ...crossed.map((p) => Math.abs(p.dist - targetDist)));
        assert.ok(worst <= 0.3, `passa do ponto em ${worst.toFixed(2)} m (de ${dist0} m)`);
      }
    });
  }
}
