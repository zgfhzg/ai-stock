import React from "react";
import ReactDOM from "react-dom/client";
import { Activity, Bot, CircleDollarSign, Plus, ShieldCheck, Trash2, Wifi } from "lucide-react";
import "./styles.css";

type SystemStatus = {
  api: string;
  trading_mode: string;
  live_trading_enabled: boolean;
  kis: {
    configured: boolean;
    base_url: string;
    account_configured: boolean;
  };
  strategy: {
    status: string;
    service: string;
  };
  risk: {
    max_order_amount_krw: number;
    max_position_ratio: number;
    daily_max_loss_ratio: number;
    daily_max_order_count: number;
  };
};

type KisApiResponse = {
  rt_cd?: string;
  msg_cd?: string;
  msg1?: string;
  output?: Record<string, string> | null;
  output1?: Array<Record<string, string>> | null;
  output2?: Array<Record<string, string>> | null;
};

type WatchlistItem = {
  symbol: string;
  name: string;
};

type DashboardData = {
  balance: KisApiResponse | null;
  quotes: Record<string, KisApiResponse>;
};

const apiBaseUrl =
  import.meta.env.VITE_API_BASE_URL ||
  `${window.location.protocol}//${window.location.hostname}:8080`;

function App() {
  const [status, setStatus] = React.useState<SystemStatus | null>(null);
  const [dashboardData, setDashboardData] = React.useState<DashboardData>({
    balance: null,
    quotes: {},
  });
  const [watchlist, setWatchlist] = React.useState<WatchlistItem[]>([]);
  const [symbolInput, setSymbolInput] = React.useState("");
  const [nameInput, setNameInput] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const accountSummary = dashboardData.balance?.output2?.[0];

  React.useEffect(() => {
    fetch(`${apiBaseUrl}/api/status`)
      .then((response) => {
        if (!response.ok) {
          throw new Error("API status request failed");
        }
        return response.json();
      })
      .then(setStatus)
      .catch(() => setError("API 서버에 연결할 수 없습니다."));
  }, []);

  React.useEffect(() => {
    if (!status?.kis?.configured || !status.kis.account_configured) {
      return;
    }

    loadDashboardData();
  }, [status]);

  function loadDashboardData() {
    Promise.all([
      fetchJson<KisApiResponse>("/api/account/balance"),
      fetchJson<WatchlistItem[]>("/api/watchlist"),
    ])
      .then(async ([balance, items]) => {
        const quotes = await loadQuotes(items);
        setWatchlist(items);
        setDashboardData({ balance, quotes });
      })
      .catch(() => setError("한국투자증권 데이터를 불러오지 못했습니다."));
  }

  function handleAddWatchlistItem(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const symbol = symbolInput.trim();
    if (!symbol) {
      return;
    }

    fetchJson<WatchlistItem[]>("/api/watchlist", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ symbol, name: nameInput.trim() || undefined }),
    })
      .then(async (items) => {
        const quotes = await loadQuotes(items);
        setWatchlist(items);
        setDashboardData((current) => ({ ...current, quotes }));
        setSymbolInput("");
        setNameInput("");
      })
      .catch(() => setError("관심종목을 저장하지 못했습니다. 6자리 종목코드를 확인하세요."));
  }

  function handleRemoveWatchlistItem(symbol: string) {
    fetchJson<WatchlistItem[]>(`/api/watchlist/${symbol}`, { method: "DELETE" })
      .then(async (items) => {
        const quotes = await loadQuotes(items);
        setWatchlist(items);
        setDashboardData((current) => ({ ...current, quotes }));
      })
      .catch(() => setError("관심종목을 삭제하지 못했습니다."));
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <CircleDollarSign size={24} />
          <span>AI Stock</span>
        </div>
        <nav>
          <a className="active">대시보드</a>
          <a>자동매매</a>
          <a>전략</a>
          <a>주문 로그</a>
          <a>설정</a>
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">Paper Trading</p>
            <h1>자동매매 관제판</h1>
          </div>
          <button className="danger-toggle" type="button">실전 주문 OFF</button>
        </header>

        {error ? <div className="notice">{error}</div> : null}

        <section className="status-grid">
          <Metric icon={<Wifi />} label="API" value={status?.api ?? "확인 중"} />
          <Metric icon={<Bot />} label="AI 전략 엔진" value={status?.strategy.status ?? "확인 중"} />
          <Metric icon={<ShieldCheck />} label="거래 모드" value={status?.trading_mode ?? "확인 중"} />
          <Metric icon={<Activity />} label="KIS 연동" value={status?.kis?.configured ? "설정됨" : "설정 필요"} />
        </section>

        <section className="content-grid">
          <div className="panel">
            <div className="panel-header">
              <h2>계좌 요약</h2>
              <span>{status?.kis?.account_configured ? "계좌 설정됨" : "모의투자 연결 대기"}</span>
            </div>
            {accountSummary ? (
              <dl className="summary-grid">
                <div><dt>총 평가금액</dt><dd>{formatKrwText(accountSummary.tot_evlu_amt)}</dd></div>
                <div><dt>예수금</dt><dd>{formatKrwText(accountSummary.dnca_tot_amt)}</dd></div>
                <div><dt>주식 평가금액</dt><dd>{formatKrwText(accountSummary.scts_evlu_amt)}</dd></div>
                <div><dt>평가손익</dt><dd>{formatKrwText(accountSummary.evlu_pfls_smtl_amt)}</dd></div>
              </dl>
            ) : (
              <div className="empty-state">
                {status?.kis?.configured
                  ? "잔고 데이터를 불러오는 중입니다."
                  : "한국투자증권 API 키를 설정하면 잔고와 손익 정보가 표시됩니다."}
              </div>
            )}
          </div>

          <div className="panel">
            <div className="panel-header">
              <h2>관심 종목</h2>
              <span>{watchlist.length}개</span>
            </div>
            <form className="watchlist-form" onSubmit={handleAddWatchlistItem}>
              <input
                aria-label="종목코드"
                inputMode="numeric"
                maxLength={6}
                placeholder="종목코드"
                value={symbolInput}
                onChange={(event) => setSymbolInput(event.target.value)}
              />
              <input
                aria-label="종목명"
                placeholder="종목명"
                value={nameInput}
                onChange={(event) => setNameInput(event.target.value)}
              />
              <button aria-label="관심종목 추가" type="submit"><Plus size={18} /></button>
            </form>
            <div className="watchlist">
              {watchlist.map((item) => {
                const quote = dashboardData.quotes[item.symbol]?.output;
                return (
                  <article className="quote-row" key={item.symbol}>
                    <div>
                      <strong>{item.name}</strong>
                      <span>{item.symbol}</span>
                    </div>
                    <div>
                      <strong>{formatKrwText(quote?.stck_prpr)}</strong>
                      <span>{formatSignedChange(quote?.prdy_vrss, quote?.prdy_ctrt)}</span>
                    </div>
                    <button aria-label={`${item.name} 삭제`} type="button" onClick={() => handleRemoveWatchlistItem(item.symbol)}>
                      <Trash2 size={16} />
                    </button>
                  </article>
                );
              })}
            </div>
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>리스크 제한</h2>
              <span>기본 보호장치</span>
            </div>
            <ul className="risk-list compact">
              <li><span>1회 주문 한도</span><strong>{formatKrw(status?.risk.max_order_amount_krw)}</strong></li>
              <li><span>종목 최대 비중</span><strong>{formatPercent(status?.risk.max_position_ratio)}</strong></li>
              <li><span>일일 손실 제한</span><strong>{formatPercent(status?.risk.daily_max_loss_ratio)}</strong></li>
              <li><span>일일 주문 횟수</span><strong>{status?.risk.daily_max_order_count ?? "-"}</strong></li>
            </ul>
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>AI 판단 로그</h2>
              <span>추천 전용</span>
            </div>
            <div className="log-line">AI는 아직 직접 주문하지 않습니다. 전략 검증 후 승인 흐름을 붙입니다.</div>
          </div>
        </section>
      </section>
    </main>
  );
}

function Metric({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="metric">
      <div className="metric-icon">{icon}</div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function formatKrw(value?: number) {
  if (value === undefined) {
    return "-";
  }
  return new Intl.NumberFormat("ko-KR", { style: "currency", currency: "KRW", maximumFractionDigits: 0 }).format(value);
}

function formatKrwText(value?: string) {
  const numberValue = Number(value ?? "");
  if (!Number.isFinite(numberValue)) {
    return "-";
  }

  return new Intl.NumberFormat("ko-KR", { style: "currency", currency: "KRW", maximumFractionDigits: 0 }).format(numberValue);
}

function formatPercent(value?: number) {
  if (value === undefined) {
    return "-";
  }
  return `${Math.round(value * 100)}%`;
}

function formatSignedChange(value?: string, rate?: string) {
  if (!value || !rate) {
    return "전일 대비 -";
  }

  const change = Number(value);
  const prefix = change > 0 ? "+" : "";
  return `전일 대비 ${prefix}${formatKrwText(value)} (${prefix}${rate}%)`;
}

function loadQuotes(items: WatchlistItem[]) {
  return Promise.all(
    items.map((item) =>
      fetchJson<KisApiResponse>(`/api/market/price/${item.symbol}`)
        .then((quote) => [item.symbol, quote] as const)
    )
  ).then((entries) => Object.fromEntries(entries));
}

function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  return fetch(`${apiBaseUrl}${path}`, init).then((response) => {
    if (!response.ok) {
      throw new Error(`Request failed: ${path}`);
    }
    return response.json();
  });
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
