import assert from "node:assert/strict";
import test from "node:test";
import { planIsActive, priceReturn, returnLabel } from "../src/planMetrics.ts";

test("returns use an explicit entry or average purchase price", () => {
  assert.equal(returnLabel(priceReturn(248000, 240000)), "+3.33%");
  assert.equal(returnLabel(priceReturn(220000, 240000)), "-8.33%");
  assert.equal(returnLabel(priceReturn(31700, 34750)), "-8.78%");
  assert.equal(returnLabel(priceReturn(31100, 34750)), "-10.50%");
  assert.equal(returnLabel(priceReturn(0, 34750)), "—");
  assert.equal(returnLabel(priceReturn(31100, 0)), "—");
  assert.equal(priceReturn(Infinity, 1), null);
});

test("expired plans are not presented as active unless an order is pending", () => {
  assert.equal(planIsActive(100, 101, false), false);
  assert.equal(planIsActive(100, 100, false), false);
  assert.equal(planIsActive(102, 101, false), true);
  assert.equal(planIsActive(100, 101, true), true);
});
