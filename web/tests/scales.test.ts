import assert from "node:assert/strict";
import {
  FRP_BREAKS,
  computeQuantileBreaks,
  frpBucket,
  frpBucketFor,
  frpColorVar,
} from "../src/lib/scales.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function assertStrictlyIncreasing(values: readonly number[]): void {
  for (let i = 1; i < values.length; i += 1) {
    assert.ok(
      values[i] > values[i - 1],
      `expected values[${i}] (${values[i]}) > values[${i - 1}] (${values[i - 1]})`,
    );
  }
}

check("computeQuantileBreaks returns 7 strictly increasing breaks for a skewed sample", () => {
  const skewed = Array.from({ length: 128 }, (_, i) => Math.pow(1.06, i + 1));
  const breaks = computeQuantileBreaks(skewed);

  assert.equal(breaks.length, 7);
  assert.notEqual(breaks, FRP_BREAKS);
  assertStrictlyIncreasing(breaks);
  assert.ok(breaks[0] > skewed[0]);
  assert.ok(breaks[6] < skewed[skewed.length - 1]);
});

check("computeQuantileBreaks prevents a single extreme outlier from collapsing lower buckets", () => {
  const bulk = Array.from({ length: 99 }, (_, i) => 1 + i * 0.5);
  const sampleWithOutlier = [...bulk, 1_000_000];
  const breaks = computeQuantileBreaks(sampleWithOutlier);

  assert.equal(breaks.length, 7);
  assert.notEqual(breaks, FRP_BREAKS);
  assertStrictlyIncreasing(breaks);
  assert.ok(breaks[6] < 60, `upper interior break (${breaks[6]}) should remain near bulk`);

  const counts = new Array<number>(8).fill(0);
  for (const val of sampleWithOutlier) {
    counts[frpBucketFor(val, breaks) - 1] += 1;
  }
  for (let b = 0; b < 7; b += 1) {
    assert.ok(counts[b] >= 10, `expected bucket ${b + 1} to stay populated, got ${counts[b]}`);
  }
  assert.ok(counts[7] >= 10);
});

check("computeQuantileBreaks falls back to FRP_BREAKS when fewer than 64 usable values exist", () => {
  const small = Array.from({ length: 63 }, (_, i) => i + 1);
  assert.equal(computeQuantileBreaks(small), FRP_BREAKS);

  const mostlyInvalid = [
    ...Array.from({ length: 63 }, (_, i) => i + 1),
    0,
    -5,
    Number.NaN,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
  ];
  assert.equal(computeQuantileBreaks(mostlyInvalid), FRP_BREAKS);
});

check("computeQuantileBreaks falls back to FRP_BREAKS on degenerate duplicate inputs", () => {
  const constant = new Array<number>(100).fill(12.5);
  assert.equal(computeQuantileBreaks(constant), FRP_BREAKS);

  // 40 identical values collapse adjacent 8-quantile cutpoints.
  const collapsed = [
    ...new Array<number>(40).fill(10),
    ...Array.from({ length: 60 }, (_, i) => 20 + i),
  ];
  assert.equal(computeQuantileBreaks(collapsed), FRP_BREAKS);
});

check("existing FRP_BREAKS, frpBucket and frpColorVar behave unchanged", () => {
  assert.deepEqual(FRP_BREAKS, [5, 10, 20, 40, 80, 160, 320]);
  assert.equal(frpBucket(4.99), 1);
  assert.equal(frpBucket(5), 2);
  assert.equal(frpBucket(319.99), 7);
  assert.equal(frpBucket(320), 8);
  assert.equal(frpColorVar(5), "var(--frp-2)");

  const custom = computeQuantileBreaks(
    Array.from({ length: 80 }, (_, i) => i + 1),
    4,
  );
  assert.equal(custom.length, 3);
  assertStrictlyIncreasing(custom);
});
