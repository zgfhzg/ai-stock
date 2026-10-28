export function priceReturn(price: number, base: number): number | null {
  if (!Number.isFinite(price) || !Number.isFinite(base) || price <= 0 || base <= 0) return null;
  return (price / base - 1) * 100;
}

export function returnLabel(value: number | null): string {
  return value === null ? "—" : `${value > 0 ? "+" : ""}${value.toFixed(2)}%`;
}

export function planIsActive(expiresAt: number, now: number, pending: boolean): boolean {
  return pending || expiresAt > now;
}
