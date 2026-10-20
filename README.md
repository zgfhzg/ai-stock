# AI Stock

한국투자증권 Open API를 연동해 로컬에서 시작하고, 나중에 VPS로 옮길 수 있게 설계하는 자동매매 시스템입니다.

초기 목표는 Raspberry Pi + 외장 SSD에서 모의투자로 안정성을 검증하는 것입니다. 실전 주문은 기본적으로 비활성화하고, AI는 먼저 매매 제안만 생성합니다.

## 구성

- `apps/web`: 대시보드
- `apps/api`: Rust 백엔드 API
- `apps/strategy`: Python AI/전략 엔진
- `docs`: 아키텍처, 로드맵, 매매 규칙
- `data`: 로컬 DB와 로그 저장 위치

## 빠른 시작

1. `.env.example`을 참고해서 `.env`를 만듭니다.
2. 모의투자 키와 계좌 정보를 설정합니다.
3. Docker Compose로 실행합니다.

```sh
make up
```

서비스 주소:

- 대시보드: `http://127.0.0.1:3000`
- Rust API: `http://localhost:8080`
- Strategy API: `http://localhost:8090`

인앱 브라우저에서 `localhost:3000`이 이전 앱 화면을 보여주거나 계속 로딩되면 `http://127.0.0.1:3000`으로 접속합니다.

라즈베리파이에 올려서 재부팅 후 자동 시작까지 설정하려면 [Raspberry Pi 운영 가이드](docs/raspberry-pi.md)를 따릅니다.

## 운영 원칙

- 실전 주문은 기본 OFF입니다.
- AI는 초기에는 주문 권한 없이 추천만 합니다.
- 모든 주문 판단과 API 응답은 로그로 남깁니다.
- 하루 손실 한도에 도달하면 자동매매를 멈춥니다.
- Raspberry Pi와 VPS 모두 같은 Docker Compose 구조로 실행합니다.
- 운영용 웹 컨테이너는 개발 서버가 아니라 빌드된 정적 파일을 서빙합니다.

## 한국투자증권 모의투자 연동

`.env`에 한국투자증권 Open API 모의투자 값을 설정합니다.

- `KIS_APP_KEY`
- `KIS_APP_SECRET`
- `KIS_ACCOUNT_NO`: 계좌번호 앞 8자리
- `KIS_ACCOUNT_PRODUCT_CODE`: 계좌번호 뒤 2자리, 보통 `01`
- `KIS_BASE_URL`: 모의투자는 `https://openapivts.koreainvestment.com:29443`

기본 API:

- `GET /api/kis/config`: KIS 설정 상태
- `POST /api/kis/token`: 접근 토큰 발급 확인, 토큰 값은 응답에 노출하지 않음
- `GET /api/account/balance`: 국내주식 잔고 조회
- `GET /api/market/price/{symbol}`: 국내주식 현재가 조회, 예: `005930`
- `GET /api/stocks/search?q=삼성전자`: 종목명 또는 종목코드 검색
- `GET /api/risk-settings`: 주문 리스크 제한 조회
- `PUT /api/risk-settings`: 주문 리스크 제한 저장
- `GET /api/watchlist`: 관심종목 목록 조회
- `POST /api/watchlist`: 관심종목 추가, 본문 예: `{ "query": "삼성전자" }`
- `DELETE /api/watchlist/{symbol}`: 관심종목 삭제
- `GET /api/news`: 최근 수집 뉴스 조회
- `GET /api/news/events`: 유사 기사들을 사건 단위로 통합한 뉴스 이벤트 조회
- `GET /api/news/stocks`: 종목별 뉴스 이벤트, 기사 수, 출처 수 집계
- `GET /api/news/stocks/:symbol/timeline`: 종목별 통합 뉴스 이벤트와 AI 분석 근거 조회
- `GET /api/news/daily-outlooks`: 오늘 저장된 종목별 AI 전망 조회
- `POST /api/news/daily-outlooks`: 최근 24시간 뉴스 이벤트로 오늘의 AI 전망 생성
- `POST /api/news/daily-outlooks/market-data`: 전망에 현재가, 등락률, 거래량, 장중 변동성 결합
- `GET /api/news/candidates`: 뉴스에서 발견한 AI 후보 종목과 제외 사유 조회
- `GET /api/news/trade-decisions`: 오늘 저장된 뉴스 기반 모의매매 결정 조회
- `POST /api/news/trade-decisions`: AI 전망, 시장 데이터, 계좌와 위험 한도를 결합한 주문 전 검토안 생성
- `GET /api/news/performance`: 뉴스 기반 결정의 1일, 3일, 5일 성과 기록 조회
- `GET /api/news/performance/summary`: 방향 적중률, 평균 수익률, 최대 낙폭 요약
- `POST /api/news/performance/evaluate`: 평가 시각이 지난 성과 항목을 현재가로 갱신
- `GET /api/news/performance/settings`: 성과 기반 자동 제외 기준 조회
- `PUT /api/news/performance/settings`: 최소 표본, 적중률, 평균 수익률, 최대 낙폭 기준 저장
- `GET /api/news/usage`: 오늘, 최근 7일, 이번 달 AI 뉴스 분석 호출량, 토큰, 예상 비용 조회
- `GET /api/news/settings`: 뉴스 분석 유효기간, 일일 건수·비용, 배치·재시도 설정 조회
- `PUT /api/news/settings`: 뉴스 분석 처리 설정 저장
- `GET /api/news/status`: 뉴스 수집 상태와 DB 용량 조회
- `POST /api/news/collect`: 뉴스 수동 수집 및 보관기간 초과 데이터 정리
- `POST /api/news/analyze`: 미분석 뉴스 최대 한 배치를 AI로 분석
- `POST /api/orders`: 국내주식 현금 지정가 주문
- `GET /api/orders`: 최근 주문 로그 조회
- `POST /api/auto-trading/run`: 관심종목 기준 자동매매 판단 1회 실행
- `GET /api/auto-trading/runs`: 최근 자동매매 판단 로그 조회
- `GET /api/auto-trading/rules`: 조건 기반 자동매매 규칙 조회
- `POST /api/auto-trading/rules`: 조건 기반 자동매매 규칙 추가
- `POST /api/auto-trading/rules/check`: 저장된 규칙을 현재가와 비교해 1회 점검
- `GET /api/auto-trading/rules/monitor`: 백엔드 규칙 감시 상태 조회
- `GET /api/auto-trading/rules/monitor/logs`: 최근 규칙 감시 로그 조회
- `POST /api/auto-trading/rules/monitor/start`: 백엔드 규칙 감시 시작
- `POST /api/auto-trading/rules/monitor/stop`: 백엔드 규칙 감시 중지
- `DELETE /api/auto-trading/rules/{id}`: 조건 기반 자동매매 규칙 삭제

AI 자동 모니터는 뉴스 전망, 최신 시세, 보유 수량과 예산을 Strategy 서비스에 전달해 AI가 진입가·익절가·손절가·수량·유효시간을 직접 결정하도록 합니다. 사용자는 예산과 위험 한도를 설정하며, 개별 가격을 입력할 필요가 없습니다. 별도 수동 규칙 기능의 `/strategy/proposal`은 기존 등락률 규칙을 유지합니다.

뉴스 수집기는 기본적으로 평일 08:00 KST에 실행됩니다. 수집 직후 새 뉴스 분석을 완료하고 당일 종목 전망을 다시 생성해 09:00 정규장 시작 전에 자동매매 후보를 준비합니다. 실행 시각은 웹 콘솔 또는 `.env`의 `NEWS_COLLECTION_HOUR_KST`, `NEWS_COLLECTION_MINUTE_KST`로 변경할 수 있습니다. 분석 건수와 비용 한도도 이 장전 실행 시각을 기준으로 거래일마다 초기화되며, 주말에는 월요일 실행 전까지 같은 한도를 유지합니다. 원문과 이미지는 저장하지 않으며 제목, 링크, 출처, 발행시각과 최대 500자의 피드 요약만 SQLite에 저장합니다. URL 기준 중복 제거, 하루 300건 제한, 90일 보관, DB 최대 1GB가 기본값입니다.

수집 직후 Strategy 서비스는 OpenAI Structured Outputs로 뉴스 요약, 감성, 중요도, 영향 기간과 관련 종목을 분석합니다. 기본 분석 유효기간은 최근 7일이며 오래된 미분석 뉴스는 `expired`로 정리합니다. 최신 뉴스는 10건 단위로 반복 처리하되 하루 200건 또는 예상 비용 $0.02에 도달하면 중단합니다. 실패 뉴스는 신규 뉴스와 분리된 배치에서 15분부터 지수 백오프로 최대 3회 재시도합니다. 분석 작업자는 기본 5분마다 남은 대기열을 확인합니다.

분석 유효기간, 일일 건수·비용 상한, 배치 크기, 작업 주기와 재시도 정책은 대시보드에서 변경할 수 있으며 `NEWS_ANALYSIS_SETTINGS_PATH`에 저장됩니다. 환경변수 `NEWS_ANALYSIS_FRESHNESS_DAYS`, `NEWS_ANALYSIS_DAILY_LIMIT`, `NEWS_ANALYSIS_DAILY_COST_LIMIT_USD`, `NEWS_ANALYSIS_BATCH_SIZE`, `NEWS_ANALYSIS_WORKER_INTERVAL_MINUTES`, `NEWS_ANALYSIS_RETRY_BASE_MINUTES`, `NEWS_ANALYSIS_MAX_ATTEMPTS`는 저장된 설정이 없을 때의 초기값입니다.

분석된 뉴스는 제목 핵심어, 관련 종목, 72시간 발행 구간을 기준으로 같은 사건끼리 묶습니다. 이벤트별 기사 수와 고유 출처 수를 분리하고, 종목별 감성은 기사 개수가 아니라 이벤트별 평균을 사용해 반복 보도가 신호를 부풀리지 않도록 합니다. 이벤트의 근거 신뢰도는 출처 다양성, 분석 완료율, 최고 중요도를 반영하며 매매 성공 확률을 의미하지 않습니다.

종목별 일일 AI 전망은 최근 24시간의 확인된 종목 뉴스 이벤트를 종합해 매수 후보, 매도 후보, 관망 중 하나와 신뢰도, 영향 기간, 핵심 근거를 생성합니다. 결과는 한국 시간 날짜별로 SQLite에 저장되며 화면 표시 전용입니다. 이 API는 주문 또는 자동매매 실행 코드를 호출하지 않습니다.

저장된 전망에는 KIS 현재가 응답의 현재가, 전일 대비 등락률, 누적 거래량과 장중 고가-저가 범위로 계산한 변동성을 결합할 수 있습니다. 거래량·거래대금·상승률·변동성은 AI가 함께 판단하며 고정 숫자로 일괄 차단하지 않습니다. 거래정지, 비정상 종목 상태, 시장경고 및 만료 신호는 계속 제외합니다.

AI 후보 종목은 사용자의 관심종목과 별도로, 오늘의 뉴스 전망에 등장한 확인된 국내 종목에서 자동 계산합니다. 뉴스 기반 가격 계획은 최신 시세와 함께 재평가하며, 보유 종목은 뉴스 후보에서 빠져도 계속 감시합니다. 관심종목 파일은 자동으로 변경하지 않습니다.

AI 자율 자동매매의 기본 후보군은 `AUTO_UNIVERSE_MODE=market`이며, 관심종목이 아니라 국내장 전체 뉴스에서 발견된 활성 AI 후보를 사용합니다. 한 번에 감시할 종목 수는 `AUTO_MARKET_CANDIDATE_LIMIT`로 제한해 KIS 호출량을 제어합니다. 이전 관심종목 방식이 필요하면 `AUTO_UNIVERSE_MODE=watchlist`로 되돌릴 수 있습니다.
자동 모니터는 당일 AI 전망이 없으면 최근 24시간의 분석된 뉴스로 전망을 생성합니다. 확인된 뉴스가 없는 날에도 보유 종목의 기존 익절·손절을 감시합니다. 가격 계획은 SQLite의 `ai_price_plans`에 저장하고 `GET /api/auto-trading/plans`로 조회합니다. 계획이 만료되거나 미보유 종목의 뉴스 전망이 갱신되면 재평가하며, AI 호출 실패 시 보유 종목의 기존 보호 가격을 유지합니다. `OPENAI_TRADE_MODEL`로 가격 판단 모델을 설정하며, 기본값은 `gpt-4o-mini`입니다. 호출 시도는 실패를 포함해 `AI_PLAN_DAILY_LIMIT`(기본 40회, KST 날짜 기준)로 제한하고 `ai_price_plan_calls`에 토큰 사용량을 기록합니다. 이 호출 한도와 사용량은 뉴스 분석 한도·사용량과 별도입니다.

실행기는 `TRADING_MODE=paper`, `AUTO_TRADE_MODE=paper_auto`에서 평일 09:00~15:20 KST에 지정가 모의주문을 제출합니다. AI가 `session=opening_auction`을 선택하면 08:30~09:00에도 시초가 동시호가 지정가 주문을 접수합니다. 이때 현재가 대신 KIS 호가 API의 예상체결가를 확인하며, 값이 없으면 주문을 보류합니다. 동시호가 접수는 장전 즉시 체결을 뜻하지 않습니다. 장전 시간외 종가(주문구분 05), NXT 프리마켓 및 휴장일 캘린더는 별도 기능이며 이번 범위에 포함하지 않습니다. 증권사 모의서버의 장전 접수 허용 여부는 실제 운영 환경에서 별도 확인해야 하며 미지원 시 거절 사유를 남기고 대기합니다. 매수는 AI 진입가 이하, 익절은 목표가 이상, 손절은 손절가 이하에서 실행을 시도하며 최종 수량은 AI 제안 수량과 예산·보유량·종목별 일일 한도의 작은 값입니다. 당일 첫 잔고 조회의 총평가액을 기준으로 손실 한도 도달 시 신규 매수를 중단합니다. 잔고 평가액이 없으면 신규 매수를 보류합니다.

주문 전 `submitting` 상태와 주문 메타데이터를 저장해 재시작이나 응답 유실 시 중복 제출을 막습니다. 접수된 주문번호·주문일·종목·매매 방향으로 일별 주문체결 API를 조회하며 연속조회도 처리합니다. 체결 완료는 증권사 체결량과 잔고 반영을 함께 확인합니다. 응답 유실 주문은 시각·수량·가격까지 일치하는 주문이 정확히 하나일 때만 복구합니다. 모호한 주문은 자동 재제출하지 않습니다.

미체결 잔량은 `AI_ORDER_REVIEW_SECONDS`(기본 300초)마다 AI가 유지·취소·가격 변경을 재평가합니다. 가격 변경은 원주문 잔량 취소 후 재주문 방식입니다. 취소를 보내기 전에 의도를 저장하고, 증권사 조회에서 취소가 확정되기 전에는 새 주문을 내지 않습니다. 부분 체결 후에는 남은 수량만 취소하고 체결된 포지션의 보호 가격을 유지합니다. 계획 만료, 이전 거래일 주문 또는 15:20 이후에는 잔량을 자동 취소합니다. 관리 주문의 체결분과 미체결 잔량을 기준으로 일일 매수 한도를 계산해 취소된 잔량이 재주문 예산에 중복 집계되지 않게 합니다. 상태 변화·취소·재평가는 `ai_order_events`에 기록합니다. 모의투자 미지원인 정정취소가능주문조회 대신 지원되는 일별 주문체결조회를 사용합니다. [KIS 지원 목록](https://github.com/koreainvestment/open-trading-api/blob/main/legacy/README.md), [체결 조회](https://github.com/koreainvestment/open-trading-api/blob/main/examples_llm/domestic_stock/inquire_daily_ccld/inquire_daily_ccld.py), [취소 API](https://github.com/koreainvestment/open-trading-api/blob/main/examples_llm/domestic_stock/order_rvsecncl/order_rvsecncl.py).

조건 기반 자동매매 규칙은 화면에서 추가하고, `규칙 점검 1회` 또는 `감시 시작`으로 현재가와 비교합니다. 감시는 백엔드 API 서버에서 실행되므로 브라우저를 닫아도 API 서버가 살아 있으면 선택한 주기마다 규칙을 반복 점검합니다. 감시 설정은 파일로 저장되어 API 서버가 재시작돼도 이전에 켜져 있던 감시를 자동 복구합니다. `모의 주문` 토글은 기본 OFF이며, 켜더라도 `AUTO_TRADE_MODE=paper_auto`일 때만 조건 충족 시 모의 주문을 시도합니다. 자동주문 실행 시에는 AI 판단 방향이 규칙 주문 방향과 같고 신뢰도가 `AUTO_MIN_CONFIDENCE` 이상이어야 합니다.
같은 규칙이 반복 발동하는 것을 막기 위해 기본 10분 쿨다운을 적용하며, `AUTO_RULE_COOLDOWN_SECONDS`로 조정할 수 있습니다. 자동주문은 규칙별 하루 1회로 제한하고, 종목별 일일 자동주문 금액은 `MAX_DAILY_AUTO_ORDER_AMOUNT_KRW_PER_SYMBOL`로 제한합니다.

종목 검색 목록은 `data/stocks.json`을 사용합니다. 공식 KIS 종목 마스터 기준으로 갱신하려면 아래 명령을 실행합니다.

```sh
python3 infra/scripts/update_stock_catalog.py
```

참고:

- 한국투자증권 접근토큰발급(P): `POST /oauth2/tokenP`
- 국내주식 현재가: `GET /uapi/domestic-stock/v1/quotations/inquire-price`
- 국내주식 잔고조회: `GET /uapi/domestic-stock/v1/trading/inquire-balance`
- 국내주식 현금주문: `POST /uapi/domestic-stock/v1/trading/order-cash`
