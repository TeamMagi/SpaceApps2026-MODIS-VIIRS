import assert from "node:assert/strict";
import { kRingForRadius } from "../src/components/Methods/h3Ring.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

check("the methods page shows the search ring the server uses (crates/core h3index.rs: k_ring_for_radius)", () => {
  // The Rust tests pin k_ring_for_radius(2.0) == 5 and k_ring_for_radius(0.01) == 2.
  assert.equal(kRingForRadius(2), 5);
  assert.equal(kRingForRadius(0.01), 2);
  // floor(radius * 1.15 / 0.64) + 2 across the 0.5-10 km range the settings drawer allows.
  assert.equal(kRingForRadius(0.5), 2);
  assert.equal(kRingForRadius(1), 3);
  assert.equal(kRingForRadius(5), 10);
  assert.equal(kRingForRadius(10), 19);
});

check("the ring never shrinks as the matching radius grows", () => {
  let previous = 0;
  for (let radius = 0.5; radius <= 10; radius += 0.5) {
    const k = kRingForRadius(radius);
    assert.ok(k >= previous, `k dropped at ${radius} km`);
    previous = k;
  }
});
