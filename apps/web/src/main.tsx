import React from "react";
import ReactDOM from "react-dom/client";
import {
  Activity,
  Bot,
  CircleDollarSign,
  Coins,
  Newspaper,
  PlayCircle,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  Wifi,
} from "lucide-react";
import "./styles.css";

type SystemStatus = {
  api: string;
  trading_mode: string;
  auto_trade_mode: string;
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
    auto_trading_budget_krw: number;
    max_order_amount_krw: number;
    max_daily_auto_order_amount_krw_per_symbol: number;
    max_position_ratio: number;
    daily_max_loss_ratio: number;
    daily_max_order_count: number;
    max_overseas_order_amount_usd: number;
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

type DomesticHolding = {
  pdno?: string;
  prdt_name?: string;
  hldg_qty?: string;
  ord_psbl_qty?: string;
  pchs_avg_pric?: string;
  prpr?: string;
  pchs_amt?: string;
  evlu_amt?: string;
  evlu_pfls_amt?: string;
  evlu_pfls_rt?: string;
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
  quantity: number;
  order_amount_krw: number;
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

type AutoMonitorStatus = {
  running: boolean;
  interval_seconds: number;
  last_started_at_unix?: number | null;
  last_stopped_at_unix?: number | null;
  last_check_at_unix?: number | null;
  next_check_at_unix?: number | null;
  last_error?: string | null;
  consecutive_error_count: number;
  last_response?: AutoRunResponse | null;
};

type NewsArticle = {
  id: number;
  title: string;
  url: string;
  source: string;
  published_at_unix: number;
  collected_at_unix: number;
  summary?: string | null;
  analysis_status: "pending" | "analyzed" | "failed";
  ai_summary?: string | null;
  sentiment?: "positive" | "neutral" | "negative" | null;
  sentiment_score?: number | null;
  importance?: number | null;
  impact_horizon?: string | null;
  related_stocks: Array<{ name?: string; symbol?: string; relevance?: number }>;
  rationale?: string | null;
  analysis_model?: string | null;
  analyzed_at_unix?: number | null;
  analysis_error?: string | null;
};

type NewsCollectorStatus = {
  running: boolean;
  collecting: boolean;
  interval_hours: number;
  retention_days: number;
  daily_limit: number;
  article_count: number;
  database_bytes: number;
  max_database_bytes: number;
  last_started_at_unix?: number | null;
  last_finished_at_unix?: number | null;
  next_collection_at_unix?: number | null;
  last_inserted_count: number;
  last_error?: string | null;
  analyzed_count: number;
  pending_analysis_count: number;
  failed_analysis_count: number;
  last_analysis_error?: string | null;
  analysis_enabled: boolean;
};

type NewsAnalysisRun = {
  requested: number;
  analyzed: number;
  failed: number;
  model?: string | null;
  elapsed_ms: number;
  input_tokens?: number | null;
  output_tokens?: number | null;
  total_tokens?: number | null;
  estimated_cost_usd?: number | null;
};

type StockNewsGroup = {
  symbol: string;
  name: string;
  market: string;
  article_count: number;
  event_count: number;
  source_count: number;
  latest_published_at_unix: number;
  average_sentiment_score?: number | null;
  articles: NewsArticle[];
};

type NewsEventGroup = {
  event_key: string;
  headline: string;
  article_count: number;
  source_count: number;
  sources: string[];
  latest_published_at_unix: number;
  average_sentiment_score?: number | null;
  max_importance?: number | null;
  evidence_confidence: number;
  related_stocks: NewsArticle["related_stocks"];
  articles: NewsArticle[];
};

type DailyStockOutlook = {
  day_key: number;
  symbol: string;
  name: string;
  action: "buy_candidate" | "sell_candidate" | "hold";
  confidence: number;
  impact_horizon: string;
  reasons: string[];
  event_keys: string[];
  model: string;
  generated_at_unix: number;
  current_price?: number | null;
  previous_change_rate?: number | null;
  accumulated_volume?: number | null;
  intraday_volatility?: number | null;
  market_checked_at_unix?: number | null;
  signal_expires_at_unix: number;
  decision_status: "market_data_pending" | "eligible" | "blocked" | "expired" | "market_data_unavailable";
  block_reason?: string | null;
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
  ai_action?: string | null;
  ai_confidence?: number | null;
  ai_reason?: string | null;
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
  consecutive_error_count: number;
  last_response?: RuleCheckResponse | null;
};

type RuleCheckLog = {
  timestamp_unix: number;
  response: RuleCheckResponse;
};

type MarketTab = "stocks" | "overseas-stocks" | "crypto-spot" | "crypto-futures";

type OverseasInstrument = {
  symbol: string;
  name: string;
  exchange_code: string;
  order_exchange_code: string;
  market: string;
  currency: string;
};

type OverseasQuote = OverseasInstrument & {
  output?: Record<string, string> | null;
};

type OverseasOrderResponse = {
  accepted: boolean;
  mode: string;
  side: string;
  symbol: string;
  exchange_code: string;
  quantity: number;
  price: number;
  order_amount_usd: number;
  status: string;
  message: string;
};

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
  const [autoMonitor, setAutoMonitor] = React.useState<AutoMonitorStatus | null>(null);
  const [autoMonitorIntervalSeconds, setAutoMonitorIntervalSeconds] = React.useState(30);
  const [autoMonitorBusy, setAutoMonitorBusy] = React.useState(false);
  const [newsEvents, setNewsEvents] = React.useState<NewsEventGroup[]>([]);
  const [newsStatus, setNewsStatus] = React.useState<NewsCollectorStatus | null>(null);
  const [lastNewsAnalysisRun, setLastNewsAnalysisRun] = React.useState<NewsAnalysisRun | null>(null);
  const [stockNewsGroups, setStockNewsGroups] = React.useState<StockNewsGroup[]>([]);
  const [newsCollecting, setNewsCollecting] = React.useState(false);
  const [newsAnalyzing, setNewsAnalyzing] = React.useState(false);
  const [dailyOutlooks, setDailyOutlooks] = React.useState<DailyStockOutlook[]>([]);
  const [dailyOutlooksGenerating, setDailyOutlooksGenerating] = React.useState(false);
  const [outlookMarketRefreshing, setOutlookMarketRefreshing] = React.useState(false);
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
    auto_trading_budget_krw: "500000",
    max_order_amount_krw: "100000",
    max_daily_auto_order_amount_krw_per_symbol: "100000",
    max_position_ratio: "20",
    daily_max_loss_ratio: "3",
    daily_max_order_count: "20",
    max_overseas_order_amount_usd: "100",
    max_crypto_order_amount_usdt: "100",
  });
  const [riskSaving, setRiskSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [overseasInstruments, setOverseasInstruments] = React.useState<OverseasInstrument[]>([]);
  const [overseasQuotes, setOverseasQuotes] = React.useState<Record<string, OverseasQuote>>({});
  const [overseasQuoteErrors, setOverseasQuoteErrors] = React.useState<Record<string, string>>({});
  const [overseasOrderSymbol, setOverseasOrderSymbol] = React.useState("AAPL");
  const [overseasOrderExchange, setOverseasOrderExchange] = React.useState("NASD");
  const [overseasOrderSide, setOverseasOrderSide] = React.useState("buy");
  const [overseasOrderQuantity, setOverseasOrderQuantity] = React.useState("1");
  const [overseasOrderPrice, setOverseasOrderPrice] = React.useState("");
  const [overseasOrderResult, setOverseasOrderResult] = React.useState<OverseasOrderResponse | null>(null);
  const [overseasOrderLogs, setOverseasOrderLogs] = React.useState<Array<Record<string, unknown>>>([]);
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
  const domesticHoldings = (dashboardData.balance?.output1 ?? []) as DomesticHolding[];
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
      .then(() => Promise.all([loadAutoRunLogs(), loadAutoMonitorStatus(), loadTradingRules(), loadRuleMonitorStatus(), loadRuleCheckLogs(), loadNews()]))
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
    if (activeMarket === "stocks" || activeMarket === "overseas-stocks") {
      return;
    }

    loadCryptoMarketData(cryptoMarketType);
  }, [activeMarket]);

  React.useEffect(() => {
    if (activeMarket !== "overseas-stocks") {
      return;
    }

    loadOverseasMarketData();
  }, [activeMarket]);

  React.useEffect(() => {
    const timer = window.setInterval(() => {
      loadRuleMonitorStatus();
      loadRuleCheckLogs();
      loadAutoMonitorStatus();
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

  function loadAutoMonitorStatus() {
    return fetchJson<AutoMonitorStatus>("/api/auto-trading/monitor")
      .then((monitor) => {
        setAutoMonitor(monitor);
        setAutoMonitorIntervalSeconds(monitor.interval_seconds);
        if (monitor.last_response) {
          setAutoRun(monitor.last_response);
        }
      })
      .catch(() => setAutoMonitor(null));
  }

  function loadNews() {
    return Promise.all([
      fetchJson<NewsEventGroup[]>("/api/news/events"),
      fetchJson<NewsCollectorStatus>("/api/news/status"),
      fetchJson<StockNewsGroup[]>("/api/news/stocks"),
      fetchJson<DailyStockOutlook[]>("/api/news/daily-outlooks"),
    ])
      .then(([events, collector, groups, outlooks]) => {
        setNewsEvents(events);
        setNewsStatus(collector);
        setStockNewsGroups(groups);
        setDailyOutlooks(outlooks);
      })
      .catch(() => {
        setNewsEvents([]);
        setNewsStatus(null);
        setStockNewsGroups([]);
        setDailyOutlooks([]);
      });
  }

  function handleCollectNews() {
    setNewsCollecting(true);
    setError(null);
    fetchJson<NewsCollectorStatus>("/api/news/collect", { method: "POST" })
      .then(setNewsStatus)
      .then(loadNews)
      .catch((error) => setError(error instanceof Error ? error.message : "뉴스를 수집하지 못했습니다."))
      .finally(() => setNewsCollecting(false));
  }

  function handleAnalyzeNews() {
    setNewsAnalyzing(true);
    setError(null);
    fetchJson<NewsAnalysisRun>("/api/news/analyze", { method: "POST" })
      .then((run) => {
        setLastNewsAnalysisRun(run);
        return loadNews();
      })
      .catch((error) => setError(error instanceof Error ? error.message : "뉴스 AI 분석에 실패했습니다."))
      .finally(() => setNewsAnalyzing(false));
  }

  function handleGenerateDailyOutlooks() {
    setDailyOutlooksGenerating(true);
    setError(null);
    fetchJson<DailyStockOutlook[]>("/api/news/daily-outlooks", { method: "POST" })
      .then(setDailyOutlooks)
      .catch((error) => setError(error instanceof Error ? error.message : "오늘의 AI 전망 생성에 실패했습니다."))
      .finally(() => setDailyOutlooksGenerating(false));
  }

  function handleRefreshOutlookMarketData() {
    setOutlookMarketRefreshing(true);
    setError(null);
    fetchJson<DailyStockOutlook[]>("/api/news/daily-outlooks/market-data", { method: "POST" })
      .then(setDailyOutlooks)
      .catch((error) => setError(error instanceof Error ? error.message : "전망에 가격 정보를 결합하지 못했습니다."))
      .finally(() => setOutlookMarketRefreshing(false));
  }

  function handleStartAutoMonitor() {
    setError(null);
    setAutoMonitorBusy(true);
    fetchJson<AutoMonitorStatus>("/api/auto-trading/monitor/start", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ interval_seconds: autoMonitorIntervalSeconds }),
    })
      .then(setAutoMonitor)
      .then(loadAutoRunLogs)
      .catch(() => setError("AI 자동매매를 시작하지 못했습니다. 관심종목과 모의 자동주문 설정을 확인하세요."))
      .finally(() => setAutoMonitorBusy(false));
  }

  function handleStopAutoMonitor() {
    setAutoMonitorBusy(true);
    fetchJson<AutoMonitorStatus>("/api/auto-trading/monitor/stop", { method: "POST" })
      .then(setAutoMonitor)
      .catch(() => setError("AI 자동매매를 중지하지 못했습니다."))
      .finally(() => setAutoMonitorBusy(false));
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

  function loadOverseasMarketData() {
    setError(null);

    fetchJson<OverseasInstrument[]>("/api/overseas-stocks/instruments")
      .then(async (items) => {
        const { quotes, quoteErrors } = await loadOverseasQuotes(items);
        setOverseasInstruments(items);
        setOverseasQuotes(quotes);
        setOverseasQuoteErrors(quoteErrors);
        if (items[0]) {
          setOverseasOrderSymbol(items[0].symbol);
          setOverseasOrderExchange(items[0].order_exchange_code);
        }
        return fetchJson<Array<Record<string, unknown>>>("/api/overseas-stocks/orders");
      })
      .then(setOverseasOrderLogs)
      .catch(() => setError("해외주식 시장 데이터를 불러오지 못했습니다."));
  }

  function handlePlaceOverseasOrder(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setOverseasOrderResult(null);

    fetchJson<OverseasOrderResponse>("/api/overseas-stocks/orders", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        side: overseasOrderSide,
        symbol: overseasOrderSymbol,
        exchange_code: overseasOrderExchange,
        quantity: Number(overseasOrderQuantity),
        price: Number(overseasOrderPrice),
      }),
    })
      .then((result) => {
        setOverseasOrderResult(result);
        return fetchJson<Array<Record<string, unknown>>>("/api/overseas-stocks/orders");
      })
      .then(setOverseasOrderLogs)
      .catch(() => setError("해외주식 주문을 기록하지 못했습니다. 수량, 가격, 한도를 확인하세요."));
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
            국내 주식
          </button>
          <button className={activeMarket === "overseas-stocks" ? "active" : ""} type="button" onClick={() => setActiveMarket("overseas-stocks")}>
            해외 주식
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
              : activeMarket === "overseas-stocks"
                ? "해외주식 실전 주문 OFF"
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
            icon={activeMarket === "crypto-spot" || activeMarket === "crypto-futures" ? <Coins /> : <Activity />}
            label={activeMarket === "crypto-spot" || activeMarket === "crypto-futures" ? status?.crypto.exchange ?? "거래소" : "KIS 연동"}
            value={activeMarket === "crypto-spot" || activeMarket === "crypto-futures" ? "공개 시세" : (status?.kis?.configured ? "설정됨" : "설정 필요")}
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

          <div className="panel wide news-panel">
            <div className="panel-header">
              <div className="panel-title-with-icon">
                <Newspaper size={19} />
                <h2>시장 뉴스</h2>
              </div>
              <span>{formatNewsStorage(newsStatus)}</span>
            </div>
            <div className="news-toolbar">
              <div>
                <strong>{formatNewsCollectorTitle(newsStatus)}</strong>
                <span>{formatNewsCollectorDetail(newsStatus)}</span>
                {lastNewsAnalysisRun ? <small>{formatNewsAnalysisRun(lastNewsAnalysisRun)}</small> : null}
              </div>
              <div className="news-actions">
                <button type="button" onClick={handleCollectNews} disabled={newsCollecting || newsStatus?.collecting}>
                  <RefreshCw size={17} />
                  <span>{newsCollecting || newsStatus?.collecting ? "수집 중" : "지금 수집"}</span>
                </button>
                <button type="button" onClick={handleAnalyzeNews} disabled={newsAnalyzing || !newsStatus?.analysis_enabled || !newsStatus?.pending_analysis_count}>
                  <Bot size={17} />
                  <span>{newsAnalyzing ? "분석 중" : "AI 분석"}</span>
                </button>
                <button type="button" onClick={handleGenerateDailyOutlooks} disabled={dailyOutlooksGenerating || !newsStatus?.analysis_enabled || !newsStatus?.analyzed_count}>
                  <Bot size={17} />
                  <span>{dailyOutlooksGenerating ? "전망 생성 중" : "오늘 전망"}</span>
                </button>
                <button type="button" onClick={handleRefreshOutlookMarketData} disabled={outlookMarketRefreshing || dailyOutlooks.length === 0}>
                  <RefreshCw size={17} />
                  <span>{outlookMarketRefreshing ? "가격 확인 중" : "가격 결합"}</span>
                </button>
              </div>
            </div>
            {dailyOutlooks.length > 0 ? (
              <div className="daily-outlooks" aria-label="오늘의 종목별 AI 전망">
                {dailyOutlooks.slice(0, 8).map((outlook) => (
                  <article className={`daily-outlook ${outlook.action} ${outlook.decision_status}`} key={outlook.symbol}>
                    <div>
                      <strong>{outlook.name}</strong>
                      <span>{outlook.symbol} · {formatOutlookAction(outlook.action)} · 신뢰도 {Math.round(outlook.confidence * 100)}%</span>
                    </div>
                    <p>{outlook.reasons.join(" ")}</p>
                    <div className="outlook-market-metrics">
                      <span>{outlook.current_price != null ? `${outlook.current_price.toLocaleString("ko-KR")}원` : "현재가 대기"}</span>
                      <span>{formatOutlookRate(outlook.previous_change_rate)}</span>
                      <span>{outlook.accumulated_volume != null ? `거래량 ${outlook.accumulated_volume.toLocaleString("ko-KR")}` : "거래량 대기"}</span>
                      <span>{outlook.intraday_volatility != null ? `변동성 ${outlook.intraday_volatility.toFixed(2)}%` : "변동성 대기"}</span>
                    </div>
                    <small>{formatImpactHorizon(outlook.impact_horizon)} · {formatOutlookStatus(outlook.decision_status)} · {formatExpiry(outlook.signal_expires_at_unix)}</small>
                    {outlook.block_reason ? <small className="outlook-block-reason">{outlook.block_reason}</small> : null}
                  </article>
                ))}
              </div>
            ) : null}
            {stockNewsGroups.length > 0 ? (
              <div className="stock-news-groups" aria-label="종목별 관련 뉴스">
                {stockNewsGroups.slice(0, 6).map((group) => (
                  <div className="stock-news-group" key={group.symbol}>
                    <strong>{group.name}</strong>
                    <span>{group.symbol} · 이벤트 {group.event_count}건 · 출처 {group.source_count}곳 · {formatAverageSentiment(group.average_sentiment_score)}</span>
                  </div>
                ))}
              </div>
            ) : null}
            {newsEvents.length > 0 ? (
              <div className="news-list">
                {newsEvents.slice(0, 10).map((event) => (
                  <a className="news-row" href={event.articles[0]?.url} key={event.event_key} rel="noreferrer" target="_blank">
                    <div>
                      <strong>{event.headline}</strong>
                      <span>{event.sources.join(", ")} · {formatRunTime(event.latest_published_at_unix)}</span>
                      <small>기사 {event.article_count}건 · 출처 {event.source_count}곳 · 근거 신뢰도 {Math.round(event.evidence_confidence * 100)}%</small>
                    </div>
                    <div className="news-analysis-copy">
                      <span>{event.articles[0]?.ai_summary || event.articles[0]?.summary || "AI 분석을 기다리고 있습니다."}</span>
                      {event.average_sentiment_score != null ? (
                        <small className={`news-sentiment ${sentimentClass(event.average_sentiment_score)}`}>
                          이벤트 감성 {formatAverageSentiment(event.average_sentiment_score)} · 최고 중요도 {event.max_importance ?? 1}/5
                        </small>
                      ) : null}
                      {event.related_stocks.length > 0 ? (
                        <div className="news-stock-tags">
                          {event.related_stocks.map((stock) => (
                            <small key={`${event.event_key}-${stock.symbol || stock.name}`}>{stock.name} {stock.symbol}</small>
                          ))}
                        </div>
                      ) : null}
                    </div>
                  </a>
                ))}
              </div>
            ) : (
              <div className="log-line">첫 수집이 완료되면 최근 시장 뉴스가 표시됩니다.</div>
            )}
          </div>

          <div className="panel wide">
            <div className="panel-header">
              <h2>보유 주식</h2>
              <span>{domesticHoldings.length}종목</span>
            </div>
            {domesticHoldings.length > 0 ? (
              <div className="holdings-list">
                <div className="holdings-header" aria-hidden="true">
                  <span>종목</span>
                  <span>보유 / 주문가능</span>
                  <span>평균 매수가</span>
                  <span>현재가</span>
                  <span>매입 / 평가금액</span>
                  <span>평가손익</span>
                </div>
                {domesticHoldings.map((holding) => (
                  <article className="holding-row" key={holding.pdno}>
                    <div className="holding-name">
                      <strong>{holding.prdt_name ?? "종목명 없음"}</strong>
                      <span>{holding.pdno ?? "-"}</span>
                    </div>
                    <div>
                      <small>보유 / 주문가능</small>
                      <strong>{formatQuantity(holding.hldg_qty)}주 / {formatQuantity(holding.ord_psbl_qty)}주</strong>
                    </div>
                    <div>
                      <small>평균 매수가</small>
                      <strong>{formatKrwText(holding.pchs_avg_pric)}</strong>
                    </div>
                    <div>
                      <small>현재가</small>
                      <strong>{formatKrwText(holding.prpr)}</strong>
                    </div>
                    <div>
                      <small>매입 / 평가금액</small>
                      <strong>{formatKrwText(holding.pchs_amt)} / {formatKrwText(holding.evlu_amt)}</strong>
                    </div>
                    <div className={formatProfitClass(holding.evlu_pfls_amt)}>
                      <small>평가손익</small>
                      <strong>{formatSignedKrw(holding.evlu_pfls_amt)}</strong>
                      <span>{formatSignedPercentText(holding.evlu_pfls_rt)}</span>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="empty-state">현재 보유 중인 국내 주식이 없습니다.</div>
            )}
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
                <span>AI 총 운용예산</span>
                <input
                  inputMode="numeric"
                  min="1"
                  type="number"
                  value={riskForm.auto_trading_budget_krw}
                  onChange={(event) => handleRiskFormChange("auto_trading_budget_krw", event.target.value)}
                />
              </label>
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
                <span>종목별 자동한도</span>
                <input
                  inputMode="numeric"
                  min="1"
                  type="number"
                  value={riskForm.max_daily_auto_order_amount_krw_per_symbol}
                  onChange={(event) => handleRiskFormChange("max_daily_auto_order_amount_krw_per_symbol", event.target.value)}
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
                <span>해외주식 1회 한도</span>
                <input
                  inputMode="decimal"
                  min="1"
                  type="number"
                  value={riskForm.max_overseas_order_amount_usd}
                  onChange={(event) => handleRiskFormChange("max_overseas_order_amount_usd", event.target.value)}
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
              <h2>AI 자율 자동매매</h2>
              <span>{autoMonitor?.running ? "감시 중" : "중지됨"}</span>
            </div>
            <div className="auto-trade-toolbar">
              <div className="auto-monitor-controls">
                <button type="button" className="ghost-button" onClick={handleRunAutoTrading} disabled={autoRunning || autoMonitor?.running}>
                  <PlayCircle size={18} />
                  <span>{autoRunning ? "판단 중" : "판단 1회"}</span>
                </button>
                <select
                  aria-label="AI 감시 주기"
                  disabled={autoMonitor?.running}
                  value={autoMonitorIntervalSeconds}
                  onChange={(event) => setAutoMonitorIntervalSeconds(Number(event.target.value))}
                >
                  <option value={30}>30초</option>
                  <option value={60}>1분</option>
                  <option value={300}>5분</option>
                  <option value={900}>15분</option>
                </select>
                <button
                  type="button"
                  onClick={autoMonitor?.running ? handleStopAutoMonitor : handleStartAutoMonitor}
                  disabled={autoMonitorBusy}
                >
                  <span>{autoMonitorBusy ? "처리 중" : autoMonitor?.running ? "자동매매 중지" : "자동매매 시작"}</span>
                </button>
              </div>
              <div>
                <strong>{formatAutoMonitorTitle(autoMonitor, autoRun, autoRunLogs)}</strong>
                <span>{formatAutoMonitorDetail(autoMonitor, status, riskForm.auto_trading_budget_krw)}</span>
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
                      <strong>{formatAction(decision.action)} · {Math.round(decision.confidence * 100)}% · {decision.quantity}주</strong>
                      <span>{decision.reason}{decision.order_amount_krw > 0 ? ` · ${formatKrw(decision.order_amount_krw)}` : ""}</span>
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
        ) : activeMarket === "overseas-stocks" ? (
          <OverseasWorkspace
            instruments={overseasInstruments}
            orderExchange={overseasOrderExchange}
            orderLogs={overseasOrderLogs}
            orderPrice={overseasOrderPrice}
            orderQuantity={overseasOrderQuantity}
            orderResult={overseasOrderResult}
            orderSide={overseasOrderSide}
            orderSymbol={overseasOrderSymbol}
            quoteErrors={overseasQuoteErrors}
            quotes={overseasQuotes}
            status={status}
            onOrderExchangeChange={setOverseasOrderExchange}
            onOrderPriceChange={setOverseasOrderPrice}
            onOrderQuantityChange={setOverseasOrderQuantity}
            onOrderSideChange={setOverseasOrderSide}
            onOrderSymbolChange={setOverseasOrderSymbol}
            onRefresh={loadOverseasMarketData}
            onSubmitOrder={handlePlaceOverseasOrder}
          />
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

function OverseasWorkspace({
  instruments,
  orderExchange,
  orderLogs,
  orderPrice,
  orderQuantity,
  orderResult,
  orderSide,
  orderSymbol,
  quoteErrors,
  quotes,
  status,
  onOrderExchangeChange,
  onOrderPriceChange,
  onOrderQuantityChange,
  onOrderSideChange,
  onOrderSymbolChange,
  onRefresh,
  onSubmitOrder,
}: {
  instruments: OverseasInstrument[];
  orderExchange: string;
  orderLogs: Array<Record<string, unknown>>;
  orderPrice: string;
  orderQuantity: string;
  orderResult: OverseasOrderResponse | null;
  orderSide: string;
  orderSymbol: string;
  quoteErrors: Record<string, string>;
  quotes: Record<string, OverseasQuote>;
  status: SystemStatus | null;
  onOrderExchangeChange: (value: string) => void;
  onOrderPriceChange: (value: string) => void;
  onOrderQuantityChange: (value: string) => void;
  onOrderSideChange: (value: string) => void;
  onOrderSymbolChange: (value: string) => void;
  onRefresh: () => void;
  onSubmitOrder: (event: React.FormEvent<HTMLFormElement>) => void;
}) {
  const selectedQuote = quotes[orderSymbol];
  const notional = Number(orderQuantity || 0) * Number(orderPrice || 0);

  return (
    <section className="content-grid">
      <div className="panel">
        <div className="panel-header">
          <h2>미국 주식 마켓</h2>
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
                key={`${instrument.exchange_code}-${instrument.symbol}`}
                type="button"
                onClick={() => {
                  onOrderSymbolChange(instrument.symbol);
                  onOrderExchangeChange(instrument.order_exchange_code);
                  const last = getOverseasLastPrice(quote);
                  if (last) {
                    onOrderPriceChange(last);
                  }
                }}
              >
                <div>
                  <strong>{instrument.name}</strong>
                  <span>{instrument.symbol} · {instrument.market}</span>
                </div>
                <div>
                  <strong>{formatUsdText(getOverseasLastPrice(quote))}</strong>
                  <span>{quoteError ?? formatOverseasChange(quote)}</span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="panel">
        <div className="panel-header">
          <h2>KIS 해외주식</h2>
          <span>미국 시장</span>
        </div>
        <dl className="summary-grid">
          <div><dt>시세 API</dt><dd>{selectedQuote ? "연결됨" : "대기"}</dd></div>
          <div><dt>API Key</dt><dd>{status?.kis.configured ? "설정됨" : "미설정"}</dd></div>
          <div><dt>실전 주문</dt><dd>{status?.live_trading_enabled ? "허용 가능" : "차단"}</dd></div>
          <div><dt>1회 한도</dt><dd>{formatUsd(status?.risk.max_overseas_order_amount_usd)}</dd></div>
        </dl>
      </div>

      <div className="panel wide">
        <div className="panel-header">
          <h2>해외주식 모의 주문</h2>
          <span>미국 주식 · 지정가 전용</span>
        </div>
        <form className="crypto-order-form" onSubmit={onSubmitOrder}>
          <select aria-label="주문 방향" value={orderSide} onChange={(event) => onOrderSideChange(event.target.value)}>
            <option value="buy">매수</option>
            <option value="sell">매도</option>
          </select>
          <select
            aria-label="해외주식 심볼"
            value={`${orderExchange}:${orderSymbol}`}
            onChange={(event) => {
              const [exchange, symbol] = event.target.value.split(":");
              onOrderExchangeChange(exchange);
              onOrderSymbolChange(symbol);
              const last = getOverseasLastPrice(quotes[symbol]);
              if (last) {
                onOrderPriceChange(last);
              }
            }}
          >
            {instruments.map((item) => (
              <option key={`${item.order_exchange_code}-order-${item.symbol}`} value={`${item.order_exchange_code}:${item.symbol}`}>
                {item.name} {item.symbol}
              </option>
            ))}
          </select>
          <input
            aria-label="주문 수량"
            inputMode="numeric"
            min="1"
            placeholder="수량"
            type="number"
            value={orderQuantity}
            onChange={(event) => onOrderQuantityChange(event.target.value)}
          />
          <input
            aria-label="주문 가격"
            inputMode="decimal"
            min="0"
            placeholder="USD 지정가"
            step="any"
            type="number"
            value={orderPrice}
            onChange={(event) => onOrderPriceChange(event.target.value)}
          />
          <button type="submit">기록</button>
        </form>
        <div className="order-meta">
          <span>예상 주문금액</span>
          <strong>{formatUsd(notional)}</strong>
        </div>
        {orderResult ? (
          <div className={orderResult.accepted ? "notice success" : "notice"}>
            {orderResult.message}
          </div>
        ) : null}
      </div>

      <div className="panel wide">
        <div className="panel-header">
          <h2>해외주식 리스크 제한</h2>
          <span>실거래 전 보호장치</span>
        </div>
        <ul className="risk-list compact">
          <li><span>실전 주문</span><strong>OFF</strong></li>
          <li><span>지원 시장</span><strong>NASDAQ · NYSE · AMEX</strong></li>
          <li><span>1회 주문 한도</span><strong>{formatUsd(status?.risk.max_overseas_order_amount_usd)}</strong></li>
          <li><span>통화</span><strong>USD</strong></li>
        </ul>
      </div>

      <div className="panel wide">
        <div className="panel-header">
          <h2>해외주식 주문 로그</h2>
          <span>최근 50건</span>
        </div>
        {orderLogs.length > 0 ? (
          <div className="order-log-list">
            {orderLogs.slice(0, 5).map((log, index) => (
              <div className="order-log-row" key={index}>
                <span>{formatOverseasOrderLog(log)}</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="log-line">아직 해외주식 주문 로그가 없습니다.</div>
        )}
      </div>
    </section>
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

function formatQuantity(value?: string) {
  const numberValue = Number(value ?? "");
  return Number.isFinite(numberValue) ? new Intl.NumberFormat("ko-KR").format(numberValue) : "-";
}

function formatSignedKrw(value?: string) {
  const numberValue = Number(value ?? "");
  if (!Number.isFinite(numberValue)) {
    return "-";
  }
  const prefix = numberValue > 0 ? "+" : "";
  return `${prefix}${formatKrw(numberValue)}`;
}

function formatSignedPercentText(value?: string) {
  const numberValue = Number(value ?? "");
  if (!Number.isFinite(numberValue)) {
    return "-";
  }
  const prefix = numberValue > 0 ? "+" : "";
  return `${prefix}${new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 2 }).format(numberValue)}%`;
}

function formatProfitClass(value?: string) {
  const numberValue = Number(value ?? "");
  if (numberValue > 0) return "holding-profit positive";
  if (numberValue < 0) return "holding-profit negative";
  return "holding-profit";
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

function formatUsd(value?: number) {
  if (value === undefined || !Number.isFinite(value)) {
    return "-";
  }
  return new Intl.NumberFormat("ko-KR", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2,
  }).format(value);
}

function formatUsdText(value?: string) {
  if (value === undefined || value === null || value === "") {
    return "-";
  }

  const numberValue = Number(value);
  if (!Number.isFinite(numberValue)) {
    return "-";
  }

  return formatUsd(numberValue);
}

function formatPercent(value?: number) {
  if (value === undefined) {
    return "-";
  }
  return `${Math.round(value * 100)}%`;
}

function formatRiskForm(risk: RiskSettings) {
  return {
    auto_trading_budget_krw: String(risk.auto_trading_budget_krw),
    max_order_amount_krw: String(risk.max_order_amount_krw),
    max_daily_auto_order_amount_krw_per_symbol: String(risk.max_daily_auto_order_amount_krw_per_symbol),
    max_position_ratio: String(Math.round(risk.max_position_ratio * 100)),
    daily_max_loss_ratio: String(Math.round(risk.daily_max_loss_ratio * 100)),
    daily_max_order_count: String(risk.daily_max_order_count),
    max_overseas_order_amount_usd: String(risk.max_overseas_order_amount_usd),
    max_crypto_order_amount_usdt: String(risk.max_crypto_order_amount_usdt),
  };
}

function parseRiskForm(form: ReturnType<typeof formatRiskForm>): RiskSettings | null {
  const autoTradingBudgetKrw = Number(form.auto_trading_budget_krw);
  const maxOrderAmountKrw = Number(form.max_order_amount_krw);
  const maxDailyAutoOrderAmountKrwPerSymbol = Number(form.max_daily_auto_order_amount_krw_per_symbol);
  const maxPositionRatio = Number(form.max_position_ratio) / 100;
  const dailyMaxLossRatio = Number(form.daily_max_loss_ratio) / 100;
  const dailyMaxOrderCount = Number(form.daily_max_order_count);
  const maxOverseasOrderAmountUsd = Number(form.max_overseas_order_amount_usd);
  const maxCryptoOrderAmountUsdt = Number(form.max_crypto_order_amount_usdt);

  if (
    autoTradingBudgetKrw <= 0 ||
    maxOrderAmountKrw <= 0 ||
    maxDailyAutoOrderAmountKrwPerSymbol <= 0 ||
    maxPositionRatio <= 0 ||
    dailyMaxLossRatio <= 0 ||
    dailyMaxOrderCount <= 0 ||
    maxOverseasOrderAmountUsd <= 0 ||
    maxCryptoOrderAmountUsdt <= 0 ||
    !Number.isFinite(autoTradingBudgetKrw) ||
    !Number.isFinite(maxOrderAmountKrw) ||
    !Number.isFinite(maxDailyAutoOrderAmountKrwPerSymbol) ||
    !Number.isFinite(maxPositionRatio) ||
    !Number.isFinite(dailyMaxLossRatio) ||
    !Number.isFinite(dailyMaxOrderCount) ||
    !Number.isFinite(maxOverseasOrderAmountUsd) ||
    !Number.isFinite(maxCryptoOrderAmountUsdt)
  ) {
    return null;
  }

  return {
    auto_trading_budget_krw: Math.round(autoTradingBudgetKrw),
    max_order_amount_krw: Math.round(maxOrderAmountKrw),
    max_daily_auto_order_amount_krw_per_symbol: Math.round(maxDailyAutoOrderAmountKrwPerSymbol),
    max_position_ratio: maxPositionRatio,
    daily_max_loss_ratio: dailyMaxLossRatio,
    daily_max_order_count: Math.round(dailyMaxOrderCount),
    max_overseas_order_amount_usd: maxOverseasOrderAmountUsd,
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

function formatAutoMonitorTitle(
  monitor: AutoMonitorStatus | null,
  run: AutoRunResponse | null,
  logs: AutoRunLog[],
) {
  if (monitor?.running) {
    return run ? formatAutoSummary(run) : "첫 AI 판단을 준비하고 있습니다.";
  }
  return run ? formatAutoSummary(run) : formatLastAutoRun(logs);
}

function formatAutoMonitorDetail(
  monitor: AutoMonitorStatus | null,
  status: SystemStatus | null,
  budget: string,
) {
  if (monitor?.last_error) {
    return `최근 오류: ${monitor.last_error}`;
  }
  const mode = status?.auto_trade_mode === "paper_auto" ? "모의 자동주문" : "추천 전용";
  const interval = monitor?.interval_seconds ?? 30;
  return `${mode} · 총 운용예산 ${formatKrw(Number(budget || 0))} · ${interval}초마다 관심종목 판단`;
}

function formatLastAutoRun(logs: AutoRunLog[]) {
  if (logs.length === 0) {
    return "추천 전용";
  }
  return `최근 실행 ${formatRunTime(logs[0].timestamp_unix)}`;
}

function formatNewsStorage(status: NewsCollectorStatus | null) {
  if (!status) return "저장소 확인 중";
  const megabytes = status.database_bytes / 1024 / 1024;
  const maxMegabytes = status.max_database_bytes / 1024 / 1024;
  return `${status.article_count.toLocaleString("ko-KR")}건 · ${megabytes.toFixed(megabytes >= 10 ? 0 : 1)} / ${maxMegabytes.toFixed(0)}MB`;
}

function formatNewsCollectorTitle(status: NewsCollectorStatus | null) {
  if (!status) return "뉴스 수집기 확인 중";
  if (status.collecting) return "새 뉴스를 수집하고 있습니다.";
  if (status.last_error) return "최근 수집에서 오류가 발생했습니다.";
  if (status.last_finished_at_unix) {
    return `최근 ${status.last_inserted_count}건 추가`;
  }
  return "첫 수집 대기 중";
}

function formatNewsCollectorDetail(status: NewsCollectorStatus | null) {
  if (!status) return "원문과 이미지는 저장하지 않습니다.";
  if (status.last_error) return status.last_error;
  if (!status.analysis_enabled) return "뉴스 수집 정상 · AI 분석은 OPENAI_API_KEY 설정 후 활성화됩니다.";
  if (status.last_analysis_error) return `수집 정상 · AI 분석 오류: ${status.last_analysis_error}`;
  return `${status.interval_hours}시간 간격 · 분석 ${status.analyzed_count} · 대기 ${status.pending_analysis_count} · 실패 ${status.failed_analysis_count}`;
}

function formatNewsAnalysisRun(run: NewsAnalysisRun) {
  const seconds = (run.elapsed_ms / 1000).toFixed(run.elapsed_ms >= 10_000 ? 1 : 2);
  const tokens = run.total_tokens ? `${run.total_tokens.toLocaleString("ko-KR")}토큰` : "토큰 미집계";
  const cost =
    run.estimated_cost_usd != null
      ? `예상 $${run.estimated_cost_usd.toFixed(run.estimated_cost_usd >= 0.01 ? 4 : 6)}`
      : "비용 미집계";
  return `최근 AI 분석 ${run.analyzed}/${run.requested}건 · ${seconds}초 · ${tokens} · ${cost}`;
}

function formatNewsAnalysisStatus(article: NewsArticle) {
  if (article.analysis_status === "analyzed") return "AI 분석 완료";
  if (article.analysis_status === "failed") return "분석 재시도 대기";
  return "분석 대기";
}

function formatNewsSentiment(sentiment?: string | null) {
  if (sentiment === "positive") return "긍정";
  if (sentiment === "negative") return "부정";
  return "중립";
}

function formatImpactHorizon(horizon?: string | null) {
  const labels: Record<string, string> = {
    intraday: "당일 영향",
    short_term: "단기 영향",
    medium_term: "중기 영향",
    long_term: "장기 영향",
    unknown: "영향기간 불명",
  };
  return labels[horizon ?? "unknown"] ?? "영향기간 불명";
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

function formatAverageSentiment(score?: number | null) {
  if (score == null) return "감성 미집계";
  if (score >= 0.25) return "긍정 우세";
  if (score <= -0.25) return "부정 우세";
  return "중립";
}

function sentimentClass(score: number) {
  if (score >= 0.25) return "positive";
  if (score <= -0.25) return "negative";
  return "neutral";
}

function formatOutlookAction(action: DailyStockOutlook["action"]) {
  if (action === "buy_candidate") return "매수 후보";
  if (action === "sell_candidate") return "매도 후보";
  return "관망";
}

function formatOutlookStatus(status: DailyStockOutlook["decision_status"]) {
  const labels: Record<DailyStockOutlook["decision_status"], string> = {
    market_data_pending: "가격 확인 대기",
    eligible: "조건 통과",
    blocked: "과열 차단",
    expired: "신호 만료",
    market_data_unavailable: "가격 확인 실패",
  };
  return labels[status];
}

function formatOutlookRate(rate?: number | null) {
  if (rate == null) return "등락률 대기";
  return `전일 대비 ${rate > 0 ? "+" : ""}${rate.toFixed(2)}%`;
}

function formatExpiry(timestampUnix: number) {
  return `유효 ${formatRunTime(timestampUnix)}까지`;
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
  if (tab === "overseas-stocks") {
    return "US Stocks";
  }
  if (tab === "crypto-spot") {
    return "Crypto Spot";
  }
  if (tab === "crypto-futures") {
    return "Crypto Futures";
  }
  return "Paper Trading";
}

function formatMarketTitle(tab: MarketTab) {
  if (tab === "overseas-stocks") {
    return "해외주식 관제판";
  }
  if (tab === "crypto-spot") {
    return "코인 현물 관제판";
  }
  if (tab === "crypto-futures") {
    return "코인 선물 관제판";
  }
  return "자동매매 관제판";
}

function getOverseasLastPrice(quote?: OverseasQuote) {
  return pickOverseasField(quote, ["last", "last_price", "ovrs_now_pric", "stck_prpr", "base"]);
}

function formatOverseasChange(quote?: OverseasQuote) {
  if (!quote?.output) {
    return "시세 대기";
  }

  const change = pickOverseasField(quote, ["diff", "prdy_vrss", "ovrs_prdy_vrss"]);
  const rate = pickOverseasField(quote, ["rate", "prdy_ctrt", "ovrs_prdy_ctrt"]);
  if (!change && !rate) {
    return `${quote.market} · ${quote.currency}`;
  }

  const changeNumber = Number(change ?? "0");
  const prefix = changeNumber > 0 ? "+" : "";
  return `전일 대비 ${change ? `${prefix}${formatUsdText(change)}` : "-"}${rate ? ` (${prefix}${rate}%)` : ""}`;
}

function pickOverseasField(quote: OverseasQuote | undefined, keys: string[]) {
  if (!quote?.output) {
    return undefined;
  }

  for (const key of keys) {
    const value = quote.output[key];
    if (value !== undefined && value !== null && value !== "") {
      return value;
    }
  }

  return undefined;
}

function formatOverseasOrderLog(log: Record<string, unknown>) {
  const response = log.response as OverseasOrderResponse | undefined;
  if (!response) {
    return "해외주식 주문 로그를 표시할 수 없습니다.";
  }

  const side = response.side === "buy" ? "매수" : "매도";
  return `${response.status} · ${side} ${response.symbol} ${response.quantity}주 @ ${formatUsd(response.price)}`;
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

async function loadOverseasQuotes(items: OverseasInstrument[]) {
  const quotes: Record<string, OverseasQuote> = {};
  const quoteErrors: Record<string, string> = {};

  for (const [index, item] of items.entries()) {
    if (index > 0) {
      await delay(450);
    }

    try {
      quotes[item.symbol] = await fetchJsonWithRetry<OverseasQuote>(
        `/api/overseas-stocks/quote/${item.exchange_code}/${item.symbol}`,
      );
    } catch (error) {
      quoteErrors[item.symbol] = error instanceof Error ? error.message : "시세 조회 실패";
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
