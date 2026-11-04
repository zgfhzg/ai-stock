import assert from "node:assert/strict";
import test from "node:test";
import { accountProfit } from "../src/accountMetrics.ts";

test("total profit includes account-wide gains, not only holdings", () => {
  assert.deepEqual(accountProfit("9996258", 10000000), {amount:-3742, percent:-0.03742});
  assert.deepEqual(accountProfit("11000000", 10000000), {amount:1000000, percent:10});
  assert.deepEqual(accountProfit("0", 100), {amount:-100, percent:-100});
});
test("missing capital or balance does not invent a return", () => {
  for (const total of [undefined, "", " ", "bad", "Infinity"]) assert.equal(accountProfit(total, 100), null);
  for (const principal of [null, 0, -1, Infinity]) assert.equal(accountProfit("100", principal), null);
});
