import assert from "node:assert/strict";
import test from "node:test";
import { isRouteHash, methodsHash, parseRoute } from "../src/lib/route";

test("dashboard state hashes are not routes", () => {
  assert.equal(isRouteHash("#region=Turkey&mode=raw"), false);
  assert.equal(isRouteHash(""), false);
  assert.deepEqual(parseRoute("#region=Turkey&mode=raw"), { name: "dashboard" });
  assert.deepEqual(parseRoute(""), { name: "dashboard" });
});

test("methods route with and without a known section", () => {
  assert.equal(isRouteHash("#/methods"), true);
  assert.deepEqual(parseRoute("#/methods"), { name: "methods", section: null });
  assert.deepEqual(parseRoute("#/methods/vza"), { name: "methods", section: "vza" });
  assert.deepEqual(parseRoute("#/methods/nope"), { name: "methods", section: null });
  assert.deepEqual(parseRoute("#/methodsx"), { name: "dashboard" });
});

test("methodsHash round-trips through parseRoute", () => {
  assert.deepEqual(parseRoute(methodsHash("limits")), { name: "methods", section: "limits" });
  assert.deepEqual(parseRoute(methodsHash()), { name: "methods", section: null });
});
