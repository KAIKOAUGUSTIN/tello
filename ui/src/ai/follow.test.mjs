import assert from 'node:assert/strict';
import { test } from 'node:test';

import { FOLLOW, FollowController, slew } from './follow.ts';
import { distanceM, torso } from './pose.ts';
import { figure } from './synthetic.ts';
import { lockTarget, Tracker } from './tracker.ts';

const person = (cx, height, extra = {}) => figure({ cx, height, ...extra });

test('tronco: centro, tamanho e estimativa sem os quadris', () => {
  const t = torso(person(0.4, 0.6));
  assert.ok(Math.abs(t.cx - 0.4) < 1e-6);
  assert.ok(Math.abs(t.len - 0.6 * 0.32) < 1e-3);
  assert.equal(t.hips, true);
  assert.ok(Math.abs(distanceM(t.len) - 2.45) < 0.05);

  const noHips = person(0.4, 0.6);
  noHips[23 * 3 + 2] = 0.1;
  const est = torso(noHips);
  assert.equal(est.hips, false);
  assert.ok(Math.abs(est.len - t.len) / t.len < 0.4, 'estimativa pela largura dos ombros');

  const noShoulders = person(0.4, 0.6);
  noShoulders[11 * 3 + 2] = 0.1;
  assert.equal(torso(noShoulders), null);
});

test('trava a maior pessoa na faixa central e recusa fora da faixa de distância', () => {
  assert.equal(lockTarget([]).ok, false);
  assert.match(lockTarget([person(0.9, 0.6)]).reason, /centro/);
  assert.match(lockTarget([person(0.5, 0.25)]).reason, /perto/);
  assert.match(lockTarget([person(0.5, 1.3, { feet: 1.1 })]).reason, /Afaste/);
  const lock = lockTarget([person(0.35, 0.45), person(0.6, 0.6)]);
  assert.ok(lock.ok);
  assert.ok(Math.abs(lock.target.cx - 0.6) < 1e-6);
});

test('o rastreador segue a mesma pessoa e conta o tempo perdido', () => {
  const lock = lockTarget([person(0.5, 0.6)]);
  const tracker = new Tracker(lock.target);
  // Outra pessoa aparece do lado; a travada anda um pouco.
  let t = tracker.update([person(0.15, 0.62), person(0.55, 0.6)], 100);
  assert.ok(Math.abs(t.cx - 0.55) < 1e-6);
  // A travada some: a outra (longe) não é adotada.
  assert.equal(tracker.update([person(0.15, 0.62)], 200), null);
  assert.equal(tracker.lostFor(1200), 1000);
  // Uma pessoa de tamanho muito diferente no mesmo lugar não conta.
  assert.equal(tracker.update([person(0.55, 0.2)], 1300), null);
  // Ela volta perto de onde sumiu.
  t = tracker.update([person(0.6, 0.6)], 1400);
  assert.ok(t);
  assert.equal(tracker.lostFor(1400), 0);
});

test('controle: zona morta, sentido e limites', () => {
  const c = new FollowController(0.19);
  assert.deepEqual(c.step({ cx: 0.52, cy: 0.5, len: 0.19, hips: true }), { yaw: 0, pitch: 0 });

  const right = new FollowController(0.19).step({ cx: 0.95, cy: 0.5, len: 0.19, hips: true });
  assert.equal(right.yaw, FOLLOW.yawMax, 'pessoa à direita: gira para a direita, no limite');

  const far = new FollowController(0.19).step({ cx: 0.5, cy: 0.5, len: 0.1, hips: true });
  assert.ok(far.pitch > 0 && far.pitch <= FOLLOW.forwardMax, 'longe: avança');

  const farNoHips = new FollowController(0.19).step({ cx: 0.5, cy: 0.5, len: 0.1, hips: false });
  assert.equal(farNoHips.pitch, 0, 'sem quadris não avança');

  const close = new FollowController(0.19).step({ cx: 0.5, cy: 0.5, len: 0.3, hips: false });
  assert.equal(close.pitch, -FOLLOW.backMax, 'perto demais: recua no máximo');

  const fast = new FollowController(0.19).step({ cx: 0.95, cy: 0.5, len: 0.3, hips: true }, true);
  assert.equal(fast.yaw, FOLLOW.yawMax / 2);
  assert.equal(fast.pitch, -FOLLOW.backMax / 2);

  assert.deepEqual(new FollowController(0.19).step(null), { yaw: 0, pitch: 0 });
});

test('limite de variação', () => {
  assert.deepEqual(slew({ yaw: 0, pitch: 0 }, { yaw: 0.35, pitch: -0.05 }, 0.1), { yaw: 0.1, pitch: -0.05 });
});

/**
 * Simulação em malha fechada: drone parado diante de uma pessoa parada, com o
 * atraso do vídeo e a taxa de análises do aparelho.
 */
function simulate({ hz, yawRate = 100, speed = 2, bearing0 = 25, dist0 = 2.5, targetDist = 2.5, fast = false }) {
  const HFOV = 70;
  const LATENCY = 0.25;
  const DT = 0.01;
  const SLEW_PER_S = 1;
  const torsoLen = (d) => 0.47 / d;
  const controller = new FollowController(torsoLen(targetDist));
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
      const cx = 0.5 + seen.bearing / HFOV;
      cmd = controller.step({ cx, cy: 0.5, len: torsoLen(seen.dist), hips: true }, fast);
    }
    out = slew(out, cmd, SLEW_PER_S * DT);
    // Resposta de primeira ordem do drone (giro rápido, translação mais lenta).
    yawVel += ((out.yaw * yawRate - yawVel) * DT) / 0.15;
    fwdVel += ((out.pitch * speed - fwdVel) * DT) / 0.4;
    bearing -= yawVel * DT;
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

for (const hz of [6, 15]) {
  for (const yawRate of [60, 100]) {
    test(`giro converge sem oscilar (${hz} análises/s, ${yawRate}°/s)`, () => {
      const trace = simulate({ hz, yawRate });
      const band = FOLLOW.yawDeadband + 0.02;
      assert.ok(settleTime(trace, (p) => Math.abs(p.cx - 0.5) <= band) < 4, 'centraliza em menos de 4 s');
      const overshoot = Math.max(...trace.map((p) => 0.5 - p.cx));
      assert.ok(overshoot <= 0.1, `ultrapassa ${overshoot.toFixed(3)}`);
    });
  }
  // Velocidade com o stick no máximo: modo normal (2 a 4 m/s) e modo rápido (8 m/s, limites pela metade).
  for (const [speed, fast] of [
    [2, false],
    [4, false],
    [8, true],
  ]) {
    test(`distância converge sem oscilar (${hz} análises/s, ${speed} m/s${fast ? ', modo rápido' : ''})`, () => {
      // Começa a 4 m de uma pessoa que deveria estar a 2,5 m (e o inverso).
      for (const [dist0, targetDist] of [
        [4, 2.5],
        [1.8, 2.5],
      ]) {
        const trace = simulate({ hz, speed, fast, bearing0: 0, dist0, targetDist });
        const okBand = (p) => Math.abs(torsoErr(p.dist, targetDist)) <= FOLLOW.pitchDeadband + 0.03;
        assert.ok(settleTime(trace, okBand) < 8, `chega à distância em menos de 8 s (de ${dist0} m)`);
        const crossed = trace.filter((p) => Math.sign(p.dist - targetDist) !== Math.sign(dist0 - targetDist));
        const worst = Math.max(0, ...crossed.map((p) => Math.abs(p.dist - targetDist)));
        assert.ok(worst <= 0.3, `passa do ponto em ${worst.toFixed(2)} m (de ${dist0} m)`);
      }
    });
  }
}

function torsoErr(dist, targetDist) {
  const len = 0.47 / dist;
  const target = 0.47 / targetDist;
  return (target - len) / target;
}
