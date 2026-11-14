import assert from "node:assert/strict";
import { performance } from "node:perf_hooks";
import {
  buildIndex,
  nearest,
  orderedForKeyboard,
  withinRadius,
} from "../src/components/Map/engine/pointIndex.ts";

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function bruteNearest(
  xs: Float32Array,
  ys: Float32Array,
  qx: number,
  qy: number,
  maxDist: number,
): number {
  let bestD2 = maxDist * maxDist;
  let bestIdx = -1;
  for (let i = 0; i < xs.length; i++) {
    const dx = xs[i] - qx;
    const dy = ys[i] - qy;
    const d2 = dx * dx + dy * dy;
    if (d2 < bestD2 || (d2 === bestD2 && (bestIdx === -1 || i < bestIdx))) {
      bestD2 = d2;
      bestIdx = i;
    }
  }
  return bestIdx;
}

function bruteWithinRadius(
  xs: Float32Array,
  ys: Float32Array,
  qx: number,
  qy: number,
  radius: number,
  maxResults: number,
): number[] {
  const r2 = radius * radius;
  const hits: { idx: number; d2: number }[] = [];
  for (let i = 0; i < xs.length; i++) {
    const dx = xs[i] - qx;
    const dy = ys[i] - qy;
    const d2 = dx * dx + dy * dy;
    if (d2 <= r2) {
      hits.push({ idx: i, d2 });
    }
  }
  hits.sort((a, b) => (a.d2 !== b.d2 ? a.d2 - b.d2 : a.idx - b.idx));
  return hits.slice(0, maxResults).map((h) => h.idx);
}

const rng = mulberry32(0x5eed1234);
const N = 5000;
const xs = new Float32Array(N);
const ys = new Float32Array(N);
for (let i = 0; i < N; i++) {
  xs[i] = -2 + rng() * 5; // [-2, 3]
  ys[i] = -1.5 + rng() * 4; // [-1.5, 2.5]
}
const idx5k = buildIndex(xs, ys);

check("nearest matches brute-force for 500 random queries (in-bounds, out-of-bounds, tiny/huge maxDist)", () => {
  assert.equal(idx5k.count, N);
  let sawMiss = false;
  let sawHit = false;
  const distChoices = [1e-4, 0.02, 0.1, 0.5, 100];

  for (let q = 0; q < 500; q++) {
    // Include queries outside [-2,3]x[-1.5,2.5] as well as inside.
    const qx = q < 100 ? (q % 2 === 0 ? -5 - rng() * 3 : 6 + rng() * 3) : -2.5 + rng() * 6;
    const qy = q < 100 ? (q % 2 === 0 ? -4 - rng() * 3 : 5 + rng() * 3) : -2.0 + rng() * 5;
    const maxDist = q < 50 ? 1e-4 : distChoices[q % distChoices.length];

    const got = nearest(idx5k, qx, qy, maxDist);
    const exp = bruteNearest(xs, ys, qx, qy, maxDist);
    assert.equal(got, exp, `query ${q} (${qx}, ${qy}, r=${maxDist}): got ${got} != ${exp}`);
    if (got === -1) sawMiss = true;
    else sawHit = true;
  }
  assert.ok(sawMiss && sawHit, "expected both hits and -1 misses");
  assert.equal(nearest(idx5k, 0, 0, -1), -1);
  assert.equal(nearest(idx5k, Number.NaN, 0, 10), -1);
});

check("withinRadius matches brute-force set and respects maxResults", () => {
  const radii = [0.005, 0.05, 0.25, 10];
  const limits = [1, 5, 32, 200];
  for (let q = 0; q < 100; q++) {
    const qx = -2.2 + rng() * 5.4;
    const qy = -1.7 + rng() * 4.4;
    const radius = radii[q % radii.length];
    const maxResults = limits[q % limits.length];

    const got = withinRadius(idx5k, qx, qy, radius, maxResults);
    const exp = bruteWithinRadius(xs, ys, qx, qy, radius, maxResults);
    assert.deepEqual(got, exp, `withinRadius mismatch at q=${q}`);
    assert.ok(got.length <= maxResults);
  }
  assert.deepEqual(withinRadius(idx5k, 100, 100, 1, 32), []);
});

check("empty index and single-point index edge cases", () => {
  const empty = buildIndex(new Float32Array(0), new Float32Array(0));
  assert.equal(empty.count, 0);
  assert.equal(nearest(empty, 0, 0, 100), -1);
  assert.deepEqual(withinRadius(empty, 0, 0, 100), []);

  const single = buildIndex(new Float32Array([1.5]), new Float32Array([-2.25]));
  assert.equal(single.count, 1);
  assert.equal(nearest(single, 1.5, -2.25, 0), 0);
  assert.equal(nearest(single, 1.6, -2.25, 0.2), 0);
  assert.equal(nearest(single, 1.6, -2.25, 0.05), -1);
  assert.deepEqual(withinRadius(single, 1.5, -2.25, 0.1), [0]);
  assert.deepEqual(withinRadius(single, 10, 10, 0.1), []);
});

check("duplicate coordinates handled deterministically by nearest and withinRadius", () => {
  const dupX = new Float32Array([0.5, 0.5, 0.5, 2.0, 0.5]);
  const dupY = new Float32Array([0.5, 0.5, 0.5, 2.0, 0.5]);
  const dupIdx = buildIndex(dupX, dupY);

  assert.equal(nearest(dupIdx, 0.5, 0.5, 0.1), 0);
  assert.deepEqual(withinRadius(dupIdx, 0.5, 0.5, 0.1, 10), [0, 1, 2, 4]);
  assert.deepEqual(withinRadius(dupIdx, 0.5, 0.5, 0.1, 2), [0, 1]);
});

check("orderedForKeyboard sorts by FRP desc, ties by id then index, accepts ArrayLike and count", () => {
  const frp = new Float32Array([10, 50, 50, 50, 20]);
  const ids = ["p0", "p-b", "p-a", "p-a", "p4"];

  // Count argument
  assert.deepEqual(orderedForKeyboard(5, frp, ids), [2, 3, 1, 4, 0]);
  assert.deepEqual(orderedForKeyboard(99, frp, ids), [2, 3, 1, 4, 0]);
  assert.deepEqual(orderedForKeyboard(0, frp, ids), []);

  // Without ids: ties broken by ascending index
  assert.deepEqual(orderedForKeyboard(5, frp), [1, 2, 3, 4, 0]);

  // ArrayLike (Uint32Array and custom ArrayLike)
  const subset = new Uint32Array([0, 1, 2, 4]);
  assert.deepEqual(orderedForKeyboard(subset, frp, ids), [2, 1, 4, 0]);
  assert.deepEqual(Array.from(subset), [0, 1, 2, 4], "input ArrayLike must not be mutated");

  const arrayLikeObj: ArrayLike<number> = { length: 3, 0: 4, 1: 1, 2: 3 };
  assert.deepEqual(orderedForKeyboard(arrayLikeObj, frp, ids), [3, 1, 4]);
});

console.log(`pointIndex.test.ts completed in ${(performance.now() - t0).toFixed(2)} ms`);
