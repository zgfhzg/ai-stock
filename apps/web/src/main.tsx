import React from "react";
import ReactDOM from "react-dom/client";
import { Activity, Bot, CircleDollarSign, ShieldCheck, Wifi } from "lucide-react";
import "./styles.css";

type SystemStatus = {
  api: string;
  trading_mode: string;
  live_trading_enabled: boolean;
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

const apiBaseUrl =
  import.meta.env.VITE_API_BASE_URL ||
  `${window.location.protocol}//${window.location.hostname}:8080`;

function App() {
  const [status, setStatus] = React.useState<SystemStatus | null>(null);
  const [error, setError] = React.useState<string | null>(null);

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
          <Metric icon={<Activity />} label="실전 주문" value={status?.live_trading_enabled ? "ON" : "OFF"} />
        </section>

        <section className="content-grid">
          <div className="panel">
            <div className="panel-header">
              <h2>계좌 요약</h2>
              <span>모의투자 연결 대기</span>
            </div>
            <div className="empty-state">한국투자증권 API 키를 설정하면 잔고와 손익 정보가 표시됩니다.</div>
          </div>

          <div className="panel">
            <div className="panel-header">
              <h2>리스크 제한</h2>
              <span>기본 보호장치</span>
            </div>
            <ul className="risk-list">
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

function formatPercent(value?: number) {
  if (value === undefined) {
    return "-";
  }
  return `${Math.round(value * 100)}%`;
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
