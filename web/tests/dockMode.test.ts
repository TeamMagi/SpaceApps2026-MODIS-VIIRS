import assert from "node:assert/strict";
import { test } from "node:test";
import { getDockMode, setDockMode, subscribeDockMode } from "../src/lib/dockMode";

test("starts in simple mode when window is undefined", () => {
  assert.equal(getDockMode(), "simple");
});

test("setDockMode updates the mode without touching a missing localStorage", () => {
  try {
    setDockMode("detailed");
    assert.equal(getDockMode(), "detailed");
  } finally {
    setDockMode("simple");
  }
});

test("subscribed listener fires once per change and not for the same value", () => {
  let calls = 0;
  const unsubscribe = subscribeDockMode(() => {
    calls += 1;
  });
  try {
    setDockMode("detailed");
    assert.equal(calls, 1);
    setDockMode("detailed");
    assert.equal(calls, 1);
  } finally {
    unsubscribe();
    setDockMode("simple");
  }
});

test("unsubscribed listener is no longer called", () => {
  let calls = 0;
  const unsubscribe = subscribeDockMode(() => {
    calls += 1;
  });
  unsubscribe();
  try {
    setDockMode("detailed");
    assert.equal(calls, 0);
  } finally {
    setDockMode("simple");
  }
});
