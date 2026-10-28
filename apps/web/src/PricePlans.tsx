import React from "react";
import { ArrowDownRight, ArrowUpRight, Clock3, RefreshCw } from "lucide-react";
import { planIsActive, priceReturn, returnLabel } from "./planMetrics";

type PricePlan = {
  plan: { symbol: string; action: string; entry_price: number; take_profit_price: number; stop_loss_price: number; quantity: number; reason: string; session?: string };
  context: { name?: string; holding_quantity?: number; average_purchase_price?: number };
  expires_at_unix: number;
  submitted_side?: string | null;
  order_status: string;
  broker_order?: { order_no: string; filled_quantity: number; remaining_quantity: number } | null;
};

const money = (value: number) => value > 0 ? `${value.toLocaleString("ko-KR")}원` : "—";
const actionLabel: Record<string, string> = { buy: "매수", sell: "매도", hold: "관망" };

export function PricePlans({ apiBaseUrl }: { apiBaseUrl: string }) {
  const [plans, setPlans] = React.useState<PricePlan[]>([]);
  const [filter, setFilter] = React.useState<"active" | "expired">("active");
  const [error, setError] = React.useState(false);
  const [loadedAt, setLoadedAt] = React.useState<number | null>(null);
  const [now, setNow] = React.useState(Date.now() / 1000);
  const [revision, setRevision] = React.useState(0);

  React.useEffect(() => {
    const controller = new AbortController();
    let busy = false;
    const load = async () => {
      if (busy) return;
      busy = true;
      try {
        const response = await fetch(`${apiBaseUrl}/api/auto-trading/plans`, { signal: controller.signal });
        if (!response.ok) throw new Error("Price plans unavailable");
        const data: PricePlan[] = await response.json();
        if (!controller.signal.aborted) {
          setPlans(data);
          setError(false);
          setLoadedAt(Date.now() / 1000);
        }
      } catch {
        if (!controller.signal.aborted) setError(true);
      } finally {
        busy = false;
        if (!controller.signal.aborted) setNow(Date.now() / 1000);
      }
    };
    void load();
    const timer = window.setInterval(() => { setNow(Date.now() / 1000); void load(); }, 15000);
    return () => { controller.abort(); window.clearInterval(timer); };
  }, [apiBaseUrl, revision]);

  const active = (item: PricePlan) => planIsActive(item.expires_at_unix, now, Boolean(item.submitted_side));
  const shown = plans.filter((item) => active(item) === (filter === "active"));

  return (
    <section className="panel wide price-plans" aria-label="AI 익절·손절 계획">
      <div className="panel-header">
        <div><h2>AI 가격 계획</h2><span className="section-caption">종목별 진입 · 익절 · 손절</span></div>
        <button className="icon-button" type="button" title="가격 계획 새로고침" aria-label="가격 계획 새로고침" onClick={() => setRevision((value) => value + 1)}><RefreshCw size={17} /></button>
      </div>
      <div className="plan-filter" aria-label="계획 상태">
        <div className="segmented-control">
          <button type="button" aria-pressed={filter === "active"} onClick={() => setFilter("active")}>유효·주문 중 <b>{plans.filter(active).length}</b></button>
          <button type="button" aria-pressed={filter === "expired"} onClick={() => setFilter("expired")}>만료 <b>{plans.filter((item) => !active(item)).length}</b></button>
        </div>
        <span className="section-caption" role="status">{error ? "조회 실패 · 마지막 수신 데이터" : loadedAt ? `${new Date(loadedAt * 1000).toLocaleTimeString("ko-KR", { timeZone: "Asia/Seoul", hour: "2-digit", minute: "2-digit" })} 갱신` : "불러오는 중"}</span>
      </div>
      {error && !loadedAt ? <div className="notice" role="alert">가격 계획을 불러오지 못했습니다.</div> : null}
      <div className="plan-table-scroll">
        <table className="plan-table">
          <thead><tr><th>종목 / 판단</th><th>진입가 / 수익률 기준</th><th><ArrowUpRight size={14} /> 익절</th><th><ArrowDownRight size={14} /> 손절</th><th>수량 / 주문</th><th>유효시간 (KST)</th></tr></thead>
          <tbody>{shown.map((item) => {
            const held = Number(item.context.holding_quantity) > 0;
            const base = held ? Number(item.context.average_purchase_price) : item.plan.entry_price;
            const expired = item.expires_at_unix <= now;
            return <tr key={item.plan.symbol}>
              <td><strong>{item.context.name || item.plan.symbol}</strong><span>{item.plan.symbol} <em className={`action-label ${item.plan.action}`}>{actionLabel[item.plan.action] || item.plan.action}</em></span><details><summary>판단 근거</summary><p>{item.plan.reason}</p></details></td>
              <td data-label="진입가 / 수익률 기준"><strong>{money(item.plan.entry_price)}</strong><span>{held ? "평균 매입가" : "계획 진입가"} {money(base)}</span></td>
              <td data-label="익절"><strong>{money(item.plan.take_profit_price)}</strong><span className={Number(priceReturn(item.plan.take_profit_price, base)) > 0 ? "positive" : "negative"}>{returnLabel(priceReturn(item.plan.take_profit_price, base))}</span></td>
              <td data-label="손절"><strong>{money(item.plan.stop_loss_price)}</strong><span className={Number(priceReturn(item.plan.stop_loss_price, base)) > 0 ? "positive" : "negative"}>{returnLabel(priceReturn(item.plan.stop_loss_price, base))}</span></td>
              <td data-label="수량 / 주문"><strong>{item.plan.quantity.toLocaleString("ko-KR")}주</strong><span>{item.broker_order ? `체결 ${item.broker_order.filled_quantity} · 잔량 ${item.broker_order.remaining_quantity}` : "미접수"}</span></td>
              <td data-label="유효시간 (KST)"><strong className={expired ? "muted" : ""}>{expired ? "만료" : new Date(item.expires_at_unix * 1000).toLocaleTimeString("ko-KR", { timeZone: "Asia/Seoul", hour: "2-digit", minute: "2-digit" })}</strong><span>{new Date(item.expires_at_unix * 1000).toLocaleDateString("ko-KR", { timeZone: "Asia/Seoul", month: "short", day: "numeric" })} · {item.plan.session === "opening_auction" ? "동시호가" : "정규장"}</span></td>
            </tr>;
          })}</tbody>
        </table>
        {!shown.length ? <div className="plan-empty"><Clock3 size={22} /><strong>{loadedAt ? filter === "active" ? "유효한 가격 계획이 없습니다" : "만료된 계획이 없습니다" : "가격 계획 조회 중"}</strong></div> : null}
      </div>
    </section>
  );
}
