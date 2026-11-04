import React from "react";
import { Save, Pencil } from "lucide-react";
import { accountProfit } from "./accountMetrics";

export function AccountPerformance({ apiBaseUrl, total }: { apiBaseUrl: string; total?: string }) {
  const [principal, setPrincipal] = React.useState<number | null>(null);
  const [draft, setDraft] = React.useState("");
  const [loaded, setLoaded] = React.useState(false);
  const [editing, setEditing] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    const controller = new AbortController();
    fetch(`${apiBaseUrl}/api/account/principal`, { signal: controller.signal })
      .then(async response => {
        if (!response.ok) throw new Error("원금을 불러오지 못했습니다.");
        const data = await response.json();
        setPrincipal(data.principal_krw);
        setDraft(data.principal_krw === null ? "" : String(data.principal_krw));
        setLoaded(true);
      }).catch(error => { if (!controller.signal.aborted) setError(error.message); });
    return () => controller.abort();
  }, [apiBaseUrl]);
  const profit = accountProfit(total, principal);
  const signed = (value: number) => `${value > 0 ? "+" : ""}${value.toLocaleString("ko-KR", { maximumFractionDigits: 0 })}원`;
  async function save(event: React.FormEvent) {
    event.preventDefault();
    const value = Number(draft);
    if (!Number.isSafeInteger(value) || value <= 0) { setError("원금을 0보다 큰 정수로 입력해 주세요."); return; }
    setBusy(true); setError(null);
    try {
      const response = await fetch(`${apiBaseUrl}/api/account/principal`, {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ principal_krw: value }),
      });
      if (!response.ok) throw new Error("원금을 저장하지 못했습니다.");
      const data = await response.json();
      setPrincipal(data.principal_krw); setEditing(false);
    } catch (error) { setError(error instanceof Error ? error.message : "저장 실패"); }
    finally { setBusy(false); }
  }
  return <section className="account-performance" aria-label="전체 원금 대비 손익">
    <dl>
      <div><dt>전체 원금 대비 손익</dt><dd className={profit ? profit.amount > 0 ? "profit-positive" : profit.amount < 0 ? "profit-negative" : "" : ""}>
        {profit ? `${signed(profit.amount)} (${profit.percent > 0 ? "+" : ""}${profit.percent.toFixed(2)}%)` : loaded ? principal === null ? "원금 미설정" : "평가금액 확인 중" : "원금 확인 중"}
      </dd></div>
      <div><dt>기준 원금 · 누적 순입금</dt><dd>{principal === null ? "-" : `${principal.toLocaleString("ko-KR")}원`}
        <button type="button" className="text-button" title="기준 원금 수정" aria-label="기준 원금 수정" disabled={!loaded || busy} onClick={() => { setDraft(principal === null ? "" : String(principal)); setEditing(!editing); }}><Pencil size={15} /></button>
      </dd></div>
    </dl>
    {loaded && (editing || principal === null) ? <form onSubmit={save}>
      <label>누적 순입금 원금 (입금 − 출금, 원)<input type="number" min="1" step="1" required value={draft} onChange={event => setDraft(event.target.value)} disabled={busy} /></label>
      <button type="submit" disabled={busy} aria-label="원금 저장" title="원금 저장"><Save size={18} /></button>
    </form> : null}
    {error ? <div role="alert">{error}</div> : null}
  </section>;
}
