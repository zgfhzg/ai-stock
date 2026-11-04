export function accountProfit(total: string | undefined, principal: number | null) {
  if (!total?.trim() || principal === null || !Number.isSafeInteger(principal) || principal <= 0) return null;
  const equity = Number(total);
  if (!Number.isFinite(equity)) return null;
  const amount = equity - principal;
  return { amount, percent: amount / principal * 100 };
}
