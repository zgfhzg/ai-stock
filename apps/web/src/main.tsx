import React from "react";
import ReactDOM from "react-dom/client";
import {
  Activity,
  Bot,
  CircleDollarSign,
  Coins,
  PlayCircle,
  Plus,
  ShieldCheck,
  Trash2,
  Wifi,
} from "lucide-react";
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
  crypto: {
    exchange: string;
    spot_base_url: string;
    futures_base_url: string;
    api_key_configured: boolean;
    api_secret_configured: boolean;
    live_trading_enabled: boolean;
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
    max_crypto_order_amount_usdt: number;
  };
};

type RiskSettings = SystemStatus["risk"];

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

type StockSearchResult = WatchlistItem & {
  market: string;
};

type DashboardData = {
  balance: KisApiResponse | null;
  quotes: Record<string, KisApiResponse>;
  quoteErrors: Record<string, string>;
};

type OrderResponse = {
  accepted: boolean;
  mode: string;
  side: string;
  symbol: string;
  quantity: number;
  price: number;
  order_amount_krw: number;
  kis: KisApiResponse;
};

type AutoDecision = {
  symbol: string;
  name: string;
  action: string;
  confidence: number;
  reason: string;
  current_price?: number | null;
  previous_change?: number | null;
  previous_change_rate?: string | null;
  order_submitted: boolean;
  skip_reason?: string | null;
};

type AutoRunResponse = {
  mode: string;
  executed: boolean;
  summary: {
    total: number;
    buy: number;
    sell: number;
    hold: number;
    skipped: number;
    orders: number;
  };
  decisions: AutoDecision[];
};

type AutoRunLog = {
  timestamp_unix: number;
  response: AutoRunResponse;
};

type TradingRuleTrigger = "buy_below" | "sell_above" | "stop_loss" | "take_profit";

type TradingRule = {
  id: string;
  symbol: string;
  name: string;
  trigger: TradingRuleTrigger;
  target_price: number;
  quantity: number;
  enabled: boolean;
  created_at_unix: number;
};

type RuleCheckResult = {
  rule_id: string;
  symbol: string;
  name: string;
  trigger: TradingRuleTrigger;
  action: string;
  status: string;
  reason: string;
  current_price?: number | null;
  target_price: number;
  quantity: number;
  order_submitted: boolean;
  cooldown_until_unix?: number | null;
};

type RuleCheckResponse = {
  mode: string;
  executed: boolean;
  summary: {
    total: number;
    matched: number;
    waiting: number;
    cooldown: number;
    skipped: number;
    orders: number;
  };
  results: RuleCheckResult[];
};

type RuleMonitorStatus = {
  running: boolean;
  interval_seconds: number;
  execute: boolean;
  last_started_at_unix?: number | null;
  last_stopped_at_unix?: number | null;
  last_check_at_unix?: number | null;
  next_check_at_unix?: number | null;
  last_error?: string | null;
  last_response?: RuleCheckResponse | null;
};

type RuleCheckLog = {
  timestamp_unix: number;
  response: RuleCheckResponse;
};

type MarketTab = "stocks" | "crypto-spot" | "crypto-futures";

type CryptoInstrument = {
  symbol: string;
  name: string;
  venue: string;
  market_type: "spot" | "futures";
  quote_asset: string;
  max_leverage?: number | null;
};

type CryptoQuote = {
  symbol: string;
  market_type: "spot" | "futures";
  venue: string;
  last_price: string;
  price_change: string;
  price_change_percent: string;
  high_price: string;
  low_price: string;
  volume: string;
  quote_volume?: string | null;
  trade_count?: number | null;
};

type CryptoOrderResponse = {
  accepted: boolean;
  mode: string;
  venue: string;
  market_type: string;
  side: string;
  symbol: string;
  quantity: number;
  price: number;
  notional_usdt: number;
  leverage?: number | null;
  status: string;
  message: string;
};

const apiBaseUrl =
  import.meta.env.VITE_API_BASE_URL ||
  `${window.location.protocol}//${window.location.hostname}:8080`;

function App() {
  const [activeMarket, setActiveMarket] = React.useState<MarketTab>("stocks");
  const [status, setStatus] = React.useState<SystemStatus | null>(null);
  const [dashboardData, setDashboardData] = React.useState<DashboardData>({
    balance: null,
    quotes: {},
    quoteErrors: {},
  });
  const [watchlist, setWatchlist] = React.useState<WatchlistItem[]>([]);
  const [stockQuery, setStockQuery] = React.useState("");
  const [stockSuggestions, setStockSuggestions] = React.useState<StockSearchResult[]>([]);
  const [orderSymbol, setOrderSymbol] = React.useState("005930");
  const [orderSide, setOrderSide] = React.useState("buy");
  const [orderQuantity, setOrderQuantity] = React.useState("1");
  const [orderPrice, setOrderPrice] = React.useState("");
  const [orderResult, setOrderResult] = React.useState<OrderResponse | null>(null);
  const [orderLogs, setOrderLogs] = React.useState<Array<Record<string, unknown>>>([]);
  const [autoRun, setAutoRun] = React.useState<AutoRunResponse | null>(null);
  const [autoRunLogs, setAutoRunLogs] = React.useState<AutoRunLog[]>([]);
  const [autoRunning, setAutoRunning] = React.useState(false);
  const [tradingRules, setTradingRules] = React.useState<TradingRule[]>([]);
  const [ruleQuery, setRuleQuery] = React.useState("");
  const [ruleTrigger, setRuleTrigger] = React.useState<TradingRuleTrigger>("buy_below");
  const [ruleTargetPrice, setRuleTargetPrice] = React.useState("");
  const [ruleQuantity, setRuleQuantity] = React.useState("1");
  const [ruleCheck, setRuleCheck] = React.useState<RuleCheckResponse | null>(null);
  const [ruleCheckLogs, setRuleCheckLogs] = React.useState<RuleCheckLog[]>([]);
  const [ruleChecking, setRuleChecking] = React.useState(false);
  const [ruleMonitor, setRuleMonitor] = React.useState<RuleMonitorStatus | null>(null);
  const [ruleMonitorIntervalSeconds, setRuleMonitorIntervalSeconds] = React.useState(30);
  const [ruleMonitorAutoOrderEnabled, setRuleMonitorAutoOrderEnabled] = React.useState(false);
  const [ruleMonitorBusy, setRuleMonitorBusy] = React.useState(false);
  const ruleCheckInFlight = React.useRef(false);
  const [riskForm, setRiskForm] = React.useState({
    max_order_amount_krw: "100000",
    max_position_ratio: "20",
    daily_max_loss_ratio: "3",
    daily_max_order_count: "20",
    max_crypto_order_amount_usdt: "100",
  });
  const [riskSaving, setRiskSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [cryptoInstruments, setCryptoInstruments] = React.useState<CryptoInstrument[]>([]);
  const [cryptoQuotes, setCryptoQuotes] = React.useState<Record<string, CryptoQuote>>({});
  const [cryptoQuoteErrors, setCryptoQuoteErrors] = React.useState<Record<string, string>>({});
  const [cryptoOrderSymbol, setCryptoOrderSymbol] = React.useState("BTCUSDT");
  const [cryptoOrderSide, setCryptoOrderSide] = React.useState("buy");
  const [cryptoOrderQuantity, setCryptoOrderQuantity] = React.useState("0.001");
  const [cryptoOrderPrice, setCryptoOrderPrice] = React.useState("");
  const [cryptoOrderLeverage, setCryptoOrderLeverage] = React.useState("1");
  const [cryptoOrderResult, setCryptoOrderResult] = React.useState<CryptoOrderResponse | null>(null);
  const [cryptoOrderLogs, setCryptoOrderLogs] = React.useState<Array<Record<string, unknown>>>([]);
  const accountSummary = dashboardData.balance?.output2?.[0];
  const cryptoMarketType = activeMarket === "crypto-futures" ? "futures" : "spot";

  React.useEffect(() => {
    fetch(`${apiBaseUrl}/api/status`)
      .then((response) => {
        if (!response.ok) {
          throw new Error("API status request failed");
        }
        return response.json();
      })
      .then(setStatus)
      .then(() => Promise.all([loadAutoRunLogs(), loadTradingRules(), loadRuleMonitorStatus(), loadRuleCheckLogs()]))
      .catch(() => setError("API 서버에 연결할 수 없습니다."));
  }, []);

  React.useEffect(() => {
    if (!status?.kis?.configured || !status.kis.account_configured) {
      return;
    }

    loadDashboardData();
  }, [status]);

  React.useEffect(() => {
    if (status?.risk) {
      setRiskForm(formatRiskForm(status.risk));
    }
  }, [status?.risk]);

  React.useEffect(() => {
    if (activeMarket === "stocks") {
      return;
    }

    loadCryptoMarketData(cryptoMarketType);
  }, [activeMarket]);

  React.useEffect(() => {
    const timer = window.setInterval(() => {
      loadRuleMonitorStatus();
      loadRuleCheckLogs();
    }, 5000);

    return () => window.clearInterval(timer);
  }, []);

  React.useEffect(() => {
    const query = stockQuery.trim();
    if (query.length < 2) {
      setStockSuggestions([]);
      return;
    }

    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      fetchJson<StockSearchResult[]>(`/api/stocks/search?q=${encodeURIComponent(query)}`, {
        signal: controller.signal,
      })
        .then(setStockSuggestions)
        .catch((error) => {
          if (error.name !== "AbortError") {
            setStockSuggestions([]);
          }
        });
    }, 180);

    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [stockQuery]);

  function loadDashboardData() {
    Promise.allSettled([
      fetchJson<KisApiResponse>("/api/account/balance"),
      fetchJson<WatchlistItem[]>("/api/watchlist"),
    ])
      .then(async ([balanceResult, watchlistResult]) => {
        const balance = balanceResult.status === "fulfilled" ? balanceResult.value : null;
        const items = watchlistResult.status === "fulfilled" ? watchlistResult.value : [];
        const { quotes, quoteErrors } = await loadQuotes(items);
        setWatchlist(items);
        if (items[0] && orderSymbol === "005930") {
          setOrderSymbol(items[0].symbol);
        }
        setDashboardData({ balance, quotes, quoteErrors });
        if (balanceResult.status === "rejected" || watchlistResult.status === "rejected") {
          setError("일부 데이터를 불러오지 못했습니다. 조회 가능한 정보는 계속 표시합니다.");
        }
        return fetchJson<Array<Record<string, unknown>>>("/api/orders");
      })
      .then(setOrderLogs)
      .catch(() => {
        setError("일부 데이터를 불러오지 못했습니다. 조회 가능한 정보는 계속 표시합니다.");
      });
  }

  function handleAddWatchlistItem(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    addWatchlistQuery(stockQuery);
  }

  function addWatchlistQuery(queryValue: string) {
    const query = queryValue.trim();
    setError(null);

    if (!query) {
      setError("추가할 종목명을 입력하세요. 예: 삼성전자");
      return;
    }

    fetchJson<WatchlistItem[]>("/api/watchlist", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ query }),
    })
      .then(async (items) => {
        const { quotes, quoteErrors } = await loadQuotes(items);
        setWatchlist(items);
        setDashboardData((current) => ({ ...current, quotes, quoteErrors }));
        setStockQuery("");
        setStockSuggestions([]);
      })
      .catch(() => setError("종목을 찾지 못했습니다. 검색 후보에서 정확한 종목을 선택해 주세요."));
  }

  function handleRemoveWatchlistItem(symbol: string) {
    fetchJson<WatchlistItem[]>(`/api/watchlist/${symbol}`, { method: "DELETE" })
      .then(async (items) => {
        const { quotes, quoteErrors } = await loadQuotes(items);
        setWatchlist(items);
        setDashboardData((current) => ({ ...current, quotes, quoteErrors }));
      })
      .catch(() => setError("관심종목을 삭제하지 못했습니다."));
  }

  function handlePlaceOrder(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setOrderResult(null);

    fetchJson<OrderResponse>("/api/orders", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        side: orderSide,
        symbol: orderSymbol,
        quantity: Number(orderQuantity),
        price: Number(orderPrice),
      }),
    })
      .then((result) => {
        setOrderResult(result);
        return fetchJson<Array<Record<string, unknown>>>("/api/orders");
      })
      .then(setOrderLogs)
      .catch(() => setError("주문을 실행하지 못했습니다. 수량, 가격, 주문 한도를 확인하세요."));
  }

  function handleRunAutoTrading() {
    setError(null);
    setAutoRunning(true);

    fetchJson<AutoRunResponse>("/api/auto-trading/run", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ execute: false }),
    })
      .then((result) => {
        setAutoRun(result);
        return loadAutoRunLogs();
      })
      .catch(() => setError("자동매매 판단을 실행하지 못했습니다. API와 전략 엔진 상태를 확인하세요."))
      .finally(() => setAutoRunning(false));
  }

  function loadAutoRunLogs() {
    return fetchJson<AutoRunLog[]>("/api/auto-trading/runs")
      .then(setAutoRunLogs)
      .catch(() => setAutoRunLogs([]));
  }

  function loadTradingRules() {
    return fetchJson<TradingRule[]>("/api/auto-trading/rules")
      .then(setTradingRules)
      .catch(() => setTradingRules([]));
  }

  function loadRuleMonitorStatus() {
    return fetchJson<RuleMonitorStatus>("/api/auto-trading/rules/monitor")
      .then((status) => {
        setRuleMonitor(status);
        setRuleMonitorIntervalSeconds(status.interval_seconds);
        setRuleMonitorAutoOrderEnabled(status.execute);
        if (status.last_response) {
          setRuleCheck(status.last_response);
        }
      })
      .catch(() => setRuleMonitor(null));
  }

  function loadRuleCheckLogs() {
    return fetchJson<RuleCheckLog[]>("/api/auto-trading/rules/monitor/logs")
      .then(setRuleCheckLogs)
      .catch(() => setRuleCheckLogs([]));
  }

  function handleAddTradingRule(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    const query = ruleQuery.trim();
    const targetPrice = Number(ruleTargetPrice);
    const quantity = Number(ruleQuantity);

    if (!query || !targetPrice || !quantity) {
      setError("종목명, 기준가, 수량을 입력하세요.");
      return;
    }

    fetchJson<TradingRule[]>("/api/auto-trading/rules", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        query,
        trigger: ruleTrigger,
        target_price: targetPrice,
        quantity,
        enabled: true,
      }),
    })
      .then((rules) => {
        setTradingRules(rules);
        setRuleQuery("");
        setRuleTargetPrice("");
        setRuleQuantity("1");
      })
      .catch(() => setError("자동매매 규칙을 추가하지 못했습니다. 종목명과 기준값을 확인하세요."));
  }

  function handleRemoveTradingRule(id: string) {
    fetchJson<TradingRule[]>(`/api/auto-trading/rules/${id}`, { method: "DELETE" })
      .then((rules) => {
        setTradingRules(rules);
        setRuleCheck(null);
      })
      .catch(() => setError("자동매매 규칙을 삭제하지 못했습니다."));
  }

  function handleCheckTradingRules() {
    runTradingRuleCheck();
  }

  function runTradingRuleCheck() {
    if (ruleCheckInFlight.current) {
      return;
    }

    ruleCheckInFlight.current = true;
    setError(null);
    setRuleChecking(true);

    fetchJson<RuleCheckResponse>("/api/auto-trading/rules/check", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ execute: false }),
    })
      .then((result) => {
        setRuleCheck(result);
        return Promise.all([loadRuleMonitorStatus(), loadRuleCheckLogs()]);
      })
      .catch(() => {
        setError("자동매매 규칙을 점검하지 못했습니다. API와 KIS 연결 상태를 확인하세요.");
      })
      .finally(() => {
        ruleCheckInFlight.current = false;
        setRuleChecking(false);
      });
  }

  function handleStartRuleMonitor() {
    setError(null);
    setRuleMonitorBusy(true);

    fetchJson<RuleMonitorStatus>("/api/auto-trading/rules/monitor/start", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        interval_seconds: ruleMonitorIntervalSeconds,
        execute: ruleMonitorAutoOrderEnabled,
      }),
    })
      .then(setRuleMonitor)
      .then(() => loadRuleCheckLogs())
      .catch(() => setError("백엔드 감시를 시작하지 못했습니다. API 상태를 확인하세요."))
      .finally(() => setRuleMonitorBusy(false));
  }

  function handleStopRuleMonitor() {
    setRuleMonitorBusy(true);

    fetchJson<RuleMonitorStatus>("/api/auto-trading/rules/monitor/stop", {
      method: "POST",
    })
      .then(setRuleMonitor)
      .catch(() => setError("백엔드 감시를 중지하지 못했습니다."))
      .finally(() => setRuleMonitorBusy(false));
  }

  function handleRiskFormChange(key: keyof typeof riskForm, value: string) {
    setRiskForm((current) => ({ ...current, [key]: value }));
  }

  function handleSaveRiskSettings(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setRiskSaving(true);

    const payload = parseRiskForm(riskForm);
    if (!payload) {
      setRiskSaving(false);
      setError("리스크 제한값은 0보다 큰 숫자로 입력하세요.");
      return;
    }

    fetchJson<RiskSettings>("/api/risk-settings", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload),
    })
      .then((risk) => {
        setStatus((current) => (current ? { ...current, risk } : current));
      })
      .catch(() => setError("리스크 제한을 저장하지 못했습니다. 입력값 범위를 확인하세요."))
      .finally(() => setRiskSaving(false));
  }

  function loadCryptoMarketData(marketType: "spot" | "futures") {
    setError(null);

    fetchJson<CryptoInstrument[]>(`/api/crypto/instruments/${marketType}`)
      .then(async (items) => {
        const { quotes, quoteErrors } = await loadCryptoQuotes(marketType, items);
        setCryptoInstruments(items);
        setCryptoQuotes(quotes);
        setCryptoQuoteErrors(quoteErrors);
        if (items[0]) {
          setCryptoOrderSymbol(items[0].symbol);
          setCryptoOrderSide(marketType === "futures" ? "long" : "buy");
        }
        return fetchJson<Array<Record<string, unknown>>>("/api/crypto/orders");
      })
      .then(setCryptoOrderLogs)
      .catch(() => setError("코인/선물 시장 데이터를 불러오지 못했습니다."));
  }

  function handlePlaceCryptoOrder(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setCryptoOrderResult(null);

    fetchJson<CryptoOrderResponse>("/api/crypto/orders", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        market_type: cryptoMarketType,
        side: cryptoOrderSide,
        symbol: cryptoOrderSymbol,
        quantity: Number(cryptoOrderQuantity),
        price: Number(cryptoOrderPrice),
        leverage: cryptoMarketType === "futures" ? Number(cryptoOrderLeverage) : undefined,
      }),
    })
      .then((result) => {
        setCryptoOrderResult(result);
        return fetchJson<Array<Record<string, unknown>>>("/api/crypto/orders");
      })
      .then(setCryptoOrderLogs)
      .catch(() => setError("코인/선물 주문을 기록하지 못했습니다. 수량, 가격, 한도를 확인하세요."));
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <CircleDollarSign size={24} />
          <span>AI Stock</span>
        </div>
        <nav>
          <button className={activeMarket === "stocks" ? "active" : ""} type="button" onClick={() => setActiveMarket("stocks")}>
            주식
          </button>
          <button className={activeMarket === "crypto-spot" ? "active" : ""} type="button" onClick={() => setActiveMarket("crypto-spot")}>
            코인 현물
          </button>
          <button className={activeMarket === "crypto-futures" ? "active" : ""} type="button" onClick={() => setActiveMarket("crypto-futures")}>
            코인 선물
          </button>
          <button type="button">전략</button>
          <button type="button">설정</button>
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{formatMarketEyebrow(activeMarket)}</p>
            <h1>{formatMarketTitle(activeMarket)}</h1>
          </div>
          <button className="danger-toggle" type="button">
            {activeMarket === "stocks"
              ? "주식 실전 주문 OFF"
              : status?.crypto.live_trading_enabled
                ? "코인 실전 주문 ON"
                : "코인 실전 주문 OFF"}
          </button>
        </header>

        {error ? <div className="notice">{error}</div> : null}

        <section className="status-grid">
          <Metric icon={<Wifi />} label="API" value={status?.api ?? "확인 중"} />
          <Metric icon={<Bot />} label="AI 전략 엔진" value={status?.strategy.status ?? "확인 중"} />
          <Metric icon={<ShieldCheck />} label="거래 모드" value={status?.trading_mode ?? "확인 중"} />
          <Metric
            icon={activeMarket === "stocks" ? <Activity /> : <Coins />}
            label={activeMarket === "stocks" ? "KIS 연동" : status?.crypto.exchange ?? "거래소"}
            value={activeMarket === "stocks" ? (status?.kis?.configured ? "설정됨" : "설정 필요") : "공개 시세"}
          />
        </section>

        {activeMarket === "stocks" ? (
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
                aria-label="종목명 또는 코드"
                placeholder="종목명 또는 코드"
                value={stockQuery}
                onChange={(event) => setStockQuery(event.target.value)}
              />
              <button aria-label="관심종목 추가" type="submit">
                <Plus size={18} />
                <span>추가</span>
              </button>
            </form>
            {stockSuggestions.length > 0 ? (
              <div className="stock-suggestions">
                {stockSuggestions.map((stock) => (
                  <button
                    aria-label={`${stock.name} 추가`}
                    key={stock.symbol}
                    type="button"
                    onClick={() => addWatchlistQuery(stock.symbol)}
                  >
                    <strong>{stock.name}</strong>
                    <span>{stock.symbol} · {stock.market}</span>
                  </button>
                ))}
              </div>
            ) : null}
            <div className="watchlist">
              {watchlist.map((item) => {
                const quote = dashboardData.quotes[item.symbol]?.output;
                const quoteError = dashboardData.quoteErrors[item.symbol];
                return (
                  <article className="quote-row" key={item.symbol}>
                    <div className="quote-main">
                      <strong>{item.name}</strong>
                      <span>{item.symbol}</span>
                    </div>
                    <div className="quote-price">
                      <strong>{formatKrwText(quote?.stck_prpr)}</strong>
                      <span>{quoteError ?? formatSignedChange(quote?.prdy_vrss, quote?.prdy_ctrt)}</span>
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
              <h2>수동 모의 주문</h2>
              <span>지정가 전용</span>
            </div>
            <form className="order-form" onSubmit={handlePlaceOrder}>
              <select aria-label="매수 매도" value={orderSide} onChange={(event) => setOrderSide(event.target.value)}>
                <option value="buy">매수</option>
                <option value="sell">매도</option>
              </select>
              <select aria-label="주문 종목" value={orderSymbol} onChange={(event) => setOrderSymbol(event.target.value)}>
                {watchlist.map((item) => (
                  <option key={item.symbol} value={item.symbol}>{item.name} {item.symbol}</option>
                ))}
              </select>
              <input
                aria-label="주문 수량"
                inputMode="numeric"
                min="1"
                placeholder="수량"
                type="number"
                value={orderQuantity}
                onChange={(event) => setOrderQuantity(event.target.value)}
              />
              <input
                aria-label="주문 가격"
                inputMode="numeric"
                min="1"
                placeholder="지정가"
                type="number"
                value={orderPrice}
                onChange={(event) => setOrderPrice(event.target.value)}
              />
              <button type="submit">주문</button>
            </form>
            <div className="order-meta">
              <span>예상 주문금액</span>
              <strong>{formatKrw(Number(orderQuantity || 0) * Number(orderPrice || 0))}</strong>
            </div>
            {orderResult ? (
              <div className={orderResult.accepted ? "notice success" : "notice"}>
                {orderResult.kis.msg1 ?? "주문 응답을 받았습니다."}
              </div>
            ) : null}
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>주문 로그</h2>
              <span>최근 50건</span>
            </div>
            {orderLogs.length > 0 ? (
              <div className="order-log-list">
                {orderLogs.slice(0, 5).map((log, index) => (
                  <div className="order-log-row" key={index}>
                    <span>{formatOrderLog(log)}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="log-line">아직 주문 로그가 없습니다.</div>
            )}
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>리스크 제한</h2>
              <span>주문 보호장치</span>
            </div>
            <form className="risk-form" onSubmit={handleSaveRiskSettings}>
              <label>
                <span>1회 주문 한도</span>
                <input
                  inputMode="numeric"
                  min="1"
                  type="number"
                  value={riskForm.max_order_amount_krw}
                  onChange={(event) => handleRiskFormChange("max_order_amount_krw", event.target.value)}
                />
              </label>
              <label>
                <span>종목 최대 비중</span>
                <input
                  inputMode="decimal"
                  max="100"
                  min="1"
                  type="number"
                  value={riskForm.max_position_ratio}
                  onChange={(event) => handleRiskFormChange("max_position_ratio", event.target.value)}
                />
              </label>
              <label>
                <span>일일 손실 제한</span>
                <input
                  inputMode="decimal"
                  max="100"
                  min="1"
                  type="number"
                  value={riskForm.daily_max_loss_ratio}
                  onChange={(event) => handleRiskFormChange("daily_max_loss_ratio", event.target.value)}
                />
              </label>
              <label>
                <span>일일 주문 횟수</span>
                <input
                  inputMode="numeric"
                  min="1"
                  type="number"
                  value={riskForm.daily_max_order_count}
                  onChange={(event) => handleRiskFormChange("daily_max_order_count", event.target.value)}
                />
              </label>
              <label>
                <span>코인 1회 한도</span>
                <input
                  inputMode="decimal"
                  min="1"
                  type="number"
                  value={riskForm.max_crypto_order_amount_usdt}
                  onChange={(event) => handleRiskFormChange("max_crypto_order_amount_usdt", event.target.value)}
                />
              </label>
              <button type="submit" disabled={riskSaving}>{riskSaving ? "저장 중" : "저장"}</button>
            </form>
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>자동매매 규칙</h2>
              <span>{tradingRules.length}개</span>
            </div>
            <form className="rule-form" onSubmit={handleAddTradingRule}>
              <input
                aria-label="규칙 종목명 또는 코드"
                placeholder="종목명 또는 코드"
                value={ruleQuery}
                onChange={(event) => setRuleQuery(event.target.value)}
              />
              <select aria-label="규칙 조건" value={ruleTrigger} onChange={(event) => setRuleTrigger(event.target.value as TradingRuleTrigger)}>
                <option value="buy_below">이하 매수</option>
                <option value="sell_above">이상 매도</option>
                <option value="stop_loss">손절</option>
                <option value="take_profit">익절</option>
              </select>
              <input
                aria-label="규칙 기준가"
                inputMode="numeric"
                min="1"
                placeholder="기준가"
                type="number"
                value={ruleTargetPrice}
                onChange={(event) => setRuleTargetPrice(event.target.value)}
              />
              <input
                aria-label="규칙 수량"
                inputMode="numeric"
                min="1"
                placeholder="수량"
                type="number"
                value={ruleQuantity}
                onChange={(event) => setRuleQuantity(event.target.value)}
              />
              <button type="submit">
                <Plus size={18} />
                <span>추가</span>
              </button>
            </form>
            {tradingRules.length > 0 ? (
              <div className="rule-list">
                {tradingRules.map((rule) => (
                  <article className="rule-row" key={rule.id}>
                    <div>
                      <strong>{rule.name}</strong>
                      <span>{rule.symbol} · {formatRuleTrigger(rule.trigger)}</span>
                    </div>
                    <div>
                      <strong>{formatKrw(rule.target_price)}</strong>
                      <span>{rule.quantity}주 · {rule.enabled ? "활성" : "비활성"}</span>
                    </div>
                    <button aria-label={`${rule.name} 규칙 삭제`} type="button" onClick={() => handleRemoveTradingRule(rule.id)}>
                      <Trash2 size={16} />
                    </button>
                  </article>
                ))}
              </div>
            ) : (
              <div className="log-line">가격 조건을 추가하면 감시 엔진에서 매매 후보로 사용합니다.</div>
            )}
            <div className="rule-check-toolbar">
              <button type="button" onClick={handleCheckTradingRules} disabled={ruleChecking || tradingRules.length === 0}>
                <PlayCircle size={18} />
                <span>{ruleChecking ? "점검 중" : "규칙 점검 1회"}</span>
              </button>
              <div>
                <strong>{ruleCheck ? formatRuleCheckSummary(ruleCheck) : "아직 점검 전"}</strong>
                <span>조건 충족 여부만 확인하고 주문은 넣지 않습니다.</span>
              </div>
            </div>
            <div className="rule-monitor-toolbar">
              <button
                className={ruleMonitor?.running ? "stop" : ""}
                type="button"
                onClick={ruleMonitor?.running ? handleStopRuleMonitor : handleStartRuleMonitor}
                disabled={ruleMonitorBusy || (tradingRules.length === 0 && !ruleMonitor?.running)}
              >
                {ruleMonitor?.running ? "감시 중지" : "감시 시작"}
              </button>
              <select
                aria-label="감시 주기"
                value={ruleMonitorIntervalSeconds}
                onChange={(event) => setRuleMonitorIntervalSeconds(Number(event.target.value))}
                disabled={ruleMonitor?.running || ruleMonitorBusy}
              >
                <option value={10}>10초</option>
                <option value={30}>30초</option>
                <option value={60}>1분</option>
                <option value={300}>5분</option>
              </select>
              <label className="rule-auto-order-toggle">
                <input
                  checked={ruleMonitorAutoOrderEnabled}
                  disabled={ruleMonitor?.running || ruleMonitorBusy}
                  type="checkbox"
                  onChange={(event) => setRuleMonitorAutoOrderEnabled(event.target.checked)}
                />
                <span>모의 주문</span>
              </label>
              <div>
                <strong>{formatRuleMonitorTitle(ruleMonitor, ruleMonitorIntervalSeconds, ruleMonitorAutoOrderEnabled)}</strong>
                <span>{formatRuleMonitorDetail(ruleMonitor, status?.trading_mode)}</span>
              </div>
            </div>
            {ruleCheck ? (
              <div className="rule-check-list">
                {ruleCheck.results.map((result) => (
                  <article className="rule-check-row" key={result.rule_id}>
                    <div>
                      <strong>{result.name}</strong>
                      <span>{formatRuleStatus(result.status)} · {formatRuleTrigger(result.trigger)}</span>
                    </div>
                    <div>
                      <strong>{formatKrw(result.current_price ?? undefined)}</strong>
                      <span>기준 {formatKrw(result.target_price)} · {result.quantity}주</span>
                    </div>
                    <span>{formatRuleCheckReason(result)}</span>
                  </article>
                ))}
              </div>
            ) : null}
            <div className="rule-log-header">
              <strong>최근 감시 로그</strong>
              <span>{ruleCheckLogs.length > 0 ? `최근 ${ruleCheckLogs.length}회` : "기록 없음"}</span>
            </div>
            {ruleCheckLogs.length > 0 ? (
              <div className="rule-log-list">
                {ruleCheckLogs.slice(0, 5).map((log, index) => (
                  <article className="rule-log-row" key={`${log.timestamp_unix}-${index}`}>
                    <div>
                      <strong>{formatRunTime(log.timestamp_unix)}</strong>
                      <span>{formatMode(log.response.mode, log.response.executed)}</span>
                    </div>
                    <div>
                      <strong>{formatRuleCheckSummary(log.response)}</strong>
                      <span>{formatRuleLogHighlight(log.response)}</span>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="log-line">감시를 시작하거나 1회 점검하면 기록이 여기에 쌓입니다.</div>
            )}
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>자동매매 실행</h2>
              <span>{autoRun ? `${autoRun.summary.total}개 판단` : formatLastAutoRun(autoRunLogs)}</span>
            </div>
            <div className="auto-trade-toolbar">
              <button type="button" onClick={handleRunAutoTrading} disabled={autoRunning}>
                <PlayCircle size={18} />
                <span>{autoRunning ? "판단 중" : "실행 1회"}</span>
              </button>
              <div>
                <strong>{autoRun ? formatAutoSummary(autoRun) : "아직 실행 전"}</strong>
                <span>기본 모드는 추천만 기록하고 주문은 넣지 않습니다.</span>
              </div>
            </div>
            {autoRun ? (
              <div className="auto-decision-list">
                {autoRun.decisions.map((decision) => (
                  <article className="auto-decision-row" key={decision.symbol}>
                    <div>
                      <strong>{decision.name}</strong>
                      <span>{decision.symbol} · {formatKrw(decision.current_price ?? undefined)}</span>
                    </div>
                    <div>
                      <strong>{formatAction(decision.action)} · {Math.round(decision.confidence * 100)}%</strong>
                      <span>{decision.reason}</span>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="log-line">관심종목 기준으로 현재가를 확인하고 AI 판단을 한 번 실행합니다.</div>
            )}
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>자동매매 판단 로그</h2>
              <span>{autoRunLogs.length > 0 ? `최근 ${autoRunLogs.length}회` : "기록 없음"}</span>
            </div>
            {autoRunLogs.length > 0 ? (
              <div className="auto-run-log-list">
                {autoRunLogs.slice(0, 5).map((log, index) => (
                  <article className="auto-run-log-row" key={`${log.timestamp_unix}-${index}`}>
                    <div className="auto-run-log-summary">
                      <strong>{formatRunTime(log.timestamp_unix)}</strong>
                      <span>{formatAutoSummary(log.response)} · {formatMode(log.response.mode, log.response.executed)}</span>
                    </div>
                    <div className="auto-run-log-decisions">
                      {log.response.decisions.slice(0, 4).map((decision) => (
                        <span key={`${log.timestamp_unix}-${decision.symbol}`}>
                          {decision.name} {formatAction(decision.action)} {Math.round(decision.confidence * 100)}%
                        </span>
                      ))}
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="log-line">자동매매 실행 기록이 아직 없습니다.</div>
            )}
          </div>
        </section>
        ) : (
          <CryptoWorkspace
            instruments={cryptoInstruments}
            marketType={cryptoMarketType}
            orderLeverage={cryptoOrderLeverage}
            orderLogs={cryptoOrderLogs}
            orderPrice={cryptoOrderPrice}
            orderQuantity={cryptoOrderQuantity}
            orderResult={cryptoOrderResult}
            orderSide={cryptoOrderSide}
            orderSymbol={cryptoOrderSymbol}
            quoteErrors={cryptoQuoteErrors}
            quotes={cryptoQuotes}
            status={status}
            onOrderLeverageChange={setCryptoOrderLeverage}
            onOrderPriceChange={setCryptoOrderPrice}
            onOrderQuantityChange={setCryptoOrderQuantity}
            onOrderSideChange={setCryptoOrderSide}
            onOrderSymbolChange={setCryptoOrderSymbol}
            onRefresh={() => loadCryptoMarketData(cryptoMarketType)}
            onSubmitOrder={handlePlaceCryptoOrder}
          />
        )}
      </section>
    </main>
  );
}

function CryptoWorkspace({
  instruments,
  marketType,
  orderLeverage,
  orderLogs,
  orderPrice,
  orderQuantity,
  orderResult,
  orderSide,
  orderSymbol,
  quoteErrors,
  quotes,
  status,
  onOrderLeverageChange,
  onOrderPriceChange,
  onOrderQuantityChange,
  onOrderSideChange,
  onOrderSymbolChange,
  onRefresh,
  onSubmitOrder,
}: {
  instruments: CryptoInstrument[];
  marketType: "spot" | "futures";
  orderLeverage: string;
  orderLogs: Array<Record<string, unknown>>;
  orderPrice: string;
  orderQuantity: string;
  orderResult: CryptoOrderResponse | null;
  orderSide: string;
  orderSymbol: string;
  quoteErrors: Record<string, string>;
  quotes: Record<string, CryptoQuote>;
  status: SystemStatus | null;
  onOrderLeverageChange: (value: string) => void;
  onOrderPriceChange: (value: string) => void;
  onOrderQuantityChange: (value: string) => void;
  onOrderSideChange: (value: string) => void;
  onOrderSymbolChange: (value: string) => void;
  onRefresh: () => void;
  onSubmitOrder: (event: React.FormEvent<HTMLFormElement>) => void;
}) {
  const selectedQuote = quotes[orderSymbol];
  const isFutures = marketType === "futures";
  const notional = Number(orderQuantity || 0) * Number(orderPrice || 0);
  const maxLeverage = instruments.reduce((max, item) => Math.max(max, item.max_leverage ?? 1), 1);

  return (
    <section className="content-grid">
      <div className="panel">
        <div className="panel-header">
          <h2>{isFutures ? "선물 마켓" : "코인 마켓"}</h2>
          <button className="ghost-button" type="button" onClick={onRefresh}>
            새로고침
          </button>
        </div>
        <div className="crypto-market-list">
          {instruments.map((instrument) => {
            const quote = quotes[instrument.symbol];
            const quoteError = quoteErrors[instrument.symbol];
            return (
              <button
                className={orderSymbol === instrument.symbol ? "crypto-market-row active" : "crypto-market-row"}
                key={`${instrument.market_type}-${instrument.symbol}`}
                type="button"
                onClick={() => {
                  onOrderSymbolChange(instrument.symbol);
                  if (quote?.last_price) {
                    onOrderPriceChange(quote.last_price);
                  }
                }}
              >
                <div>
                  <strong>{instrument.name}</strong>
                  <span>{instrument.symbol} · {instrument.venue}</span>
                </div>
                <div>
                  <strong>{quote ? formatUsdtText(quote.last_price) : "-"}</strong>
                  <span>{quoteError ?? formatCryptoChange(quote)}</span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="panel">
        <div className="panel-header">
          <h2>거래소 연결</h2>
          <span>{status?.crypto.exchange ?? "Binance"}</span>
        </div>
        <dl className="summary-grid">
          <div><dt>시세 API</dt><dd>{selectedQuote ? "연결됨" : "대기"}</dd></div>
          <div><dt>API Key</dt><dd>{status?.crypto.api_key_configured ? "설정됨" : "미설정"}</dd></div>
          <div><dt>실전 주문</dt><dd>{status?.crypto.live_trading_enabled ? "허용" : "차단"}</dd></div>
          <div><dt>1회 한도</dt><dd>{formatUsdt(status?.risk.max_crypto_order_amount_usdt)}</dd></div>
        </dl>
      </div>

      <div className="panel wide">
        <div className="panel-header">
          <h2>{isFutures ? "선물 모의 주문" : "코인 현물 모의 주문"}</h2>
          <span>{isFutures ? "USDT 무기한 · 레버리지 제한" : "지정가 전용"}</span>
        </div>
        <form className={isFutures ? "crypto-order-form futures" : "crypto-order-form"} onSubmit={onSubmitOrder}>
          <select aria-label="주문 방향" value={orderSide} onChange={(event) => onOrderSideChange(event.target.value)}>
            {isFutures ? (
              <>
                <option value="long">롱</option>
                <option value="short">숏</option>
              </>
            ) : (
              <>
                <option value="buy">매수</option>
                <option value="sell">매도</option>
              </>
            )}
          </select>
          <select aria-label="코인 심볼" value={orderSymbol} onChange={(event) => onOrderSymbolChange(event.target.value)}>
            {instruments.map((item) => (
              <option key={`${item.market_type}-order-${item.symbol}`} value={item.symbol}>
                {item.name} {item.symbol}
              </option>
            ))}
          </select>
          <input
            aria-label="주문 수량"
            inputMode="decimal"
            min="0"
            placeholder="수량"
            step="any"
            type="number"
            value={orderQuantity}
            onChange={(event) => onOrderQuantityChange(event.target.value)}
          />
          <input
            aria-label="주문 가격"
            inputMode="decimal"
            min="0"
            placeholder="USDT 지정가"
            step="any"
            type="number"
            value={orderPrice}
            onChange={(event) => onOrderPriceChange(event.target.value)}
          />
          {isFutures ? (
            <input
              aria-label="레버리지"
              inputMode="numeric"
              max="20"
              min="1"
              placeholder="레버리지"
              type="number"
              value={orderLeverage}
              onChange={(event) => onOrderLeverageChange(event.target.value)}
            />
          ) : null}
          <button type="submit">기록</button>
        </form>
        <div className="order-meta">
          <span>예상 명목금액</span>
          <strong>{formatUsdt(notional)}</strong>
        </div>
        {orderResult ? (
          <div className={orderResult.accepted ? "notice success" : "notice"}>
            {orderResult.message}
          </div>
        ) : null}
      </div>

      <div className="panel wide">
        <div className="panel-header">
          <h2>{isFutures ? "선물 리스크 제한" : "코인 리스크 제한"}</h2>
          <span>실거래 전 보호장치</span>
        </div>
        <ul className="risk-list compact">
          <li><span>실전 주문</span><strong>{status?.crypto.live_trading_enabled ? "ON" : "OFF"}</strong></li>
          <li><span>API 권한</span><strong>{status?.crypto.api_key_configured ? "키 있음" : "읽기 전용"}</strong></li>
          <li><span>1회 주문 한도</span><strong>{formatUsdt(status?.risk.max_crypto_order_amount_usdt)}</strong></li>
          <li><span>레버리지</span><strong>{isFutures ? `최대 ${maxLeverage}x` : "사용 안함"}</strong></li>
        </ul>
      </div>

      <div className="panel wide">
        <div className="panel-header">
          <h2>코인 주문 로그</h2>
          <span>최근 50건</span>
        </div>
        {orderLogs.length > 0 ? (
          <div className="order-log-list">
            {orderLogs.slice(0, 5).map((log, index) => (
              <div className="order-log-row" key={index}>
                <span>{formatCryptoOrderLog(log)}</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="log-line">아직 코인/선물 주문 로그가 없습니다.</div>
        )}
      </div>
    </section>
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
  if (value === undefined || value === null || value === "") {
    return "-";
  }

  const numberValue = Number(value ?? "");
  if (!Number.isFinite(numberValue)) {
    return "-";
  }

  return new Intl.NumberFormat("ko-KR", { style: "currency", currency: "KRW", maximumFractionDigits: 0 }).format(numberValue);
}

function formatUsdt(value?: number) {
  if (value === undefined || !Number.isFinite(value)) {
    return "-";
  }
  return `${new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 2 }).format(value)} USDT`;
}

function formatUsdtText(value?: string) {
  if (value === undefined || value === null || value === "") {
    return "-";
  }

  const numberValue = Number(value);
  if (!Number.isFinite(numberValue)) {
    return "-";
  }

  return formatUsdt(numberValue);
}

function formatPercent(value?: number) {
  if (value === undefined) {
    return "-";
  }
  return `${Math.round(value * 100)}%`;
}

function formatRiskForm(risk: RiskSettings) {
  return {
    max_order_amount_krw: String(risk.max_order_amount_krw),
    max_position_ratio: String(Math.round(risk.max_position_ratio * 100)),
    daily_max_loss_ratio: String(Math.round(risk.daily_max_loss_ratio * 100)),
    daily_max_order_count: String(risk.daily_max_order_count),
    max_crypto_order_amount_usdt: String(risk.max_crypto_order_amount_usdt),
  };
}

function parseRiskForm(form: ReturnType<typeof formatRiskForm>): RiskSettings | null {
  const maxOrderAmountKrw = Number(form.max_order_amount_krw);
  const maxPositionRatio = Number(form.max_position_ratio) / 100;
  const dailyMaxLossRatio = Number(form.daily_max_loss_ratio) / 100;
  const dailyMaxOrderCount = Number(form.daily_max_order_count);
  const maxCryptoOrderAmountUsdt = Number(form.max_crypto_order_amount_usdt);

  if (
    maxOrderAmountKrw <= 0 ||
    maxPositionRatio <= 0 ||
    dailyMaxLossRatio <= 0 ||
    dailyMaxOrderCount <= 0 ||
    maxCryptoOrderAmountUsdt <= 0 ||
    !Number.isFinite(maxOrderAmountKrw) ||
    !Number.isFinite(maxPositionRatio) ||
    !Number.isFinite(dailyMaxLossRatio) ||
    !Number.isFinite(dailyMaxOrderCount) ||
    !Number.isFinite(maxCryptoOrderAmountUsdt)
  ) {
    return null;
  }

  return {
    max_order_amount_krw: Math.round(maxOrderAmountKrw),
    max_position_ratio: maxPositionRatio,
    daily_max_loss_ratio: dailyMaxLossRatio,
    daily_max_order_count: Math.round(dailyMaxOrderCount),
    max_crypto_order_amount_usdt: maxCryptoOrderAmountUsdt,
  };
}

function formatSignedChange(value?: string, rate?: string) {
  if (!value || !rate) {
    return "전일 대비 -";
  }

  const change = Number(value);
  const prefix = change > 0 ? "+" : "";
  return `전일 대비 ${prefix}${formatKrwText(value)} (${prefix}${rate}%)`;
}

function formatOrderLog(log: Record<string, unknown>) {
  const response = log.response as OrderResponse | undefined;
  if (!response) {
    return "주문 로그를 표시할 수 없습니다.";
  }

  const side = response.side === "buy" ? "매수" : "매도";
  const status = response.accepted ? "접수" : "거절";
  return `${status} · ${side} ${response.symbol} ${response.quantity}주 @ ${formatKrw(response.price)}`;
}

function formatAutoSummary(run: AutoRunResponse) {
  return `관망 ${run.summary.hold} · 매수 ${run.summary.buy} · 매도 ${run.summary.sell} · 주문 ${run.summary.orders}`;
}

function formatLastAutoRun(logs: AutoRunLog[]) {
  if (logs.length === 0) {
    return "추천 전용";
  }
  return `최근 실행 ${formatRunTime(logs[0].timestamp_unix)}`;
}

function formatRunTime(timestampUnix: number) {
  return new Intl.DateTimeFormat("ko-KR", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestampUnix * 1000));
}

function formatClock(date: Date) {
  return new Intl.DateTimeFormat("ko-KR", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function formatMode(mode: string, executed: boolean) {
  if (executed) {
    return "모의 주문 실행";
  }
  if (mode === "paper_auto") {
    return "자동주문 대기";
  }
  return "추천만";
}

function formatRuleTrigger(trigger: TradingRuleTrigger) {
  const labels: Record<TradingRuleTrigger, string> = {
    buy_below: "가격 이하 매수",
    sell_above: "가격 이상 매도",
    stop_loss: "손절",
    take_profit: "익절",
  };
  return labels[trigger];
}

function formatRuleCheckSummary(check: RuleCheckResponse) {
  return `충족 ${check.summary.matched} · 대기 ${check.summary.waiting} · 쿨다운 ${check.summary.cooldown} · 주문 ${check.summary.orders}`;
}

function formatRuleLogHighlight(check: RuleCheckResponse) {
  const important =
    check.results.find((result) => result.status === "order_submitted") ??
    check.results.find((result) => result.status === "order_rejected") ??
    check.results.find((result) => result.status === "matched") ??
    check.results.find((result) => result.status === "cooldown") ??
    check.results.find((result) => result.status === "skipped") ??
    check.results[0];

  if (!important) {
    return "점검할 규칙이 없습니다.";
  }

  return `${important.name} · ${formatRuleStatus(important.status)} · ${formatRuleCheckReason(important)}`;
}

function formatRuleMonitorTitle(
  status: RuleMonitorStatus | null,
  selectedIntervalSeconds: number,
  autoOrderEnabled: boolean,
) {
  const suffix = autoOrderEnabled ? " · 모의 주문 ON" : " · 확인만";
  if (status?.running) {
    return `${status.interval_seconds}초마다 백엔드 감시 중${suffix}`;
  }
  return `백엔드 감시 대기 · ${selectedIntervalSeconds}초${suffix}`;
}

function formatRuleMonitorDetail(status: RuleMonitorStatus | null, tradingMode?: string) {
  if (status?.last_error) {
    return `최근 오류: ${status.last_error}`;
  }
  if (status?.execute && tradingMode !== "paper_auto") {
    return "모의 주문은 AUTO_TRADE_MODE=paper_auto에서만 실행됩니다.";
  }
  if (status?.last_check_at_unix) {
    const next = status.next_check_at_unix ? ` · 다음 ${formatClockFromUnix(status.next_check_at_unix)}` : "";
    return `마지막 점검 ${formatClockFromUnix(status.last_check_at_unix)}${next}`;
  }
  if (status?.running) {
    return "곧 첫 점검을 시작합니다.";
  }
  return "브라우저를 닫아도 API 서버가 켜져 있으면 감시할 수 있습니다.";
}

function formatClockFromUnix(timestampUnix: number) {
  return formatClock(new Date(timestampUnix * 1000));
}

function formatRuleStatus(status: string) {
  const labels: Record<string, string> = {
    matched: "조건 충족",
    waiting: "대기",
    cooldown: "쿨다운",
    skipped: "건너뜀",
    order_submitted: "주문 접수",
    order_rejected: "주문 거절",
  };
  return labels[status] ?? status;
}

function formatRuleCheckReason(result: RuleCheckResult) {
  if (result.cooldown_until_unix) {
    return `${result.reason} 해제 ${formatRunTime(result.cooldown_until_unix)}`;
  }
  return result.reason;
}

function formatAction(action: string) {
  if (action === "buy") {
    return "매수";
  }
  if (action === "sell") {
    return "매도";
  }
  if (action === "skip") {
    return "건너뜀";
  }
  return "관망";
}

function formatMarketEyebrow(tab: MarketTab) {
  if (tab === "crypto-spot") {
    return "Crypto Spot";
  }
  if (tab === "crypto-futures") {
    return "Crypto Futures";
  }
  return "Paper Trading";
}

function formatMarketTitle(tab: MarketTab) {
  if (tab === "crypto-spot") {
    return "코인 현물 관제판";
  }
  if (tab === "crypto-futures") {
    return "코인 선물 관제판";
  }
  return "자동매매 관제판";
}

function formatCryptoChange(quote?: CryptoQuote) {
  if (!quote) {
    return "시세 대기";
  }

  const change = Number(quote.price_change);
  const prefix = change > 0 ? "+" : "";
  return `24h ${prefix}${formatUsdtText(quote.price_change)} (${prefix}${quote.price_change_percent}%)`;
}

function formatCryptoOrderLog(log: Record<string, unknown>) {
  const response = log.response as CryptoOrderResponse | undefined;
  if (!response) {
    return "코인 주문 로그를 표시할 수 없습니다.";
  }

  const sideLabels: Record<string, string> = {
    buy: "매수",
    sell: "매도",
    long: "롱",
    short: "숏",
  };
  const leverage = response.leverage ? ` · ${response.leverage}x` : "";
  return `${response.status} · ${sideLabels[response.side] ?? response.side} ${response.symbol} ${response.quantity} @ ${formatUsdt(response.price)}${leverage}`;
}

async function loadQuotes(items: WatchlistItem[]) {
  const quotes: Record<string, KisApiResponse> = {};
  const quoteErrors: Record<string, string> = {};

  for (const [index, item] of items.entries()) {
    if (index > 0) {
      await delay(1200);
    }

    try {
      quotes[item.symbol] = await fetchJsonWithRetry<KisApiResponse>(`/api/market/price/${item.symbol}`);
    } catch (error) {
      quoteErrors[item.symbol] = error instanceof Error ? error.message : "현재가 조회 실패";
    }
  }

  return { quotes, quoteErrors };
}

async function loadCryptoQuotes(marketType: "spot" | "futures", items: CryptoInstrument[]) {
  const quotes: Record<string, CryptoQuote> = {};
  const quoteErrors: Record<string, string> = {};

  for (const item of items) {
    try {
      quotes[item.symbol] = await fetchJsonWithRetry<CryptoQuote>(`/api/crypto/quote/${marketType}/${item.symbol}`);
    } catch (error) {
      quoteErrors[item.symbol] = error instanceof Error ? error.message : "시세 조회 실패";
    }
  }

  return { quotes, quoteErrors };
}

function fetchJsonWithRetry<T>(path: string, attempts = 2): Promise<T> {
  return fetchJson<T>(path).catch((error) => {
    if (attempts <= 1) {
      throw error;
    }

    return new Promise<T>((resolve, reject) => {
      window.setTimeout(() => {
        fetchJsonWithRetry<T>(path, attempts - 1).then(resolve).catch(reject);
      }, 1200);
    });
  });
}

function delay(milliseconds: number) {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  return fetch(`${apiBaseUrl}${path}`, init).then((response) => {
    if (!response.ok) {
      return response.text().then((text) => {
        throw new Error(extractErrorMessage(text) || `Request failed: ${path}`);
      });
    }
    return response.json();
  });
}

function extractErrorMessage(text: string): string {
  if (!text) {
    return "";
  }

  try {
    const payload = JSON.parse(text) as { message?: string; msg1?: string };
    if (payload.msg1) {
      return payload.msg1;
    }
    if (payload.message) {
      return extractErrorMessage(payload.message) || payload.message;
    }
  } catch {
    return text.length > 80 ? "현재가 조회 제한으로 잠시 후 다시 시도해 주세요." : text;
  }

  return text.length > 80 ? "현재가 조회 제한으로 잠시 후 다시 시도해 주세요." : text;
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
