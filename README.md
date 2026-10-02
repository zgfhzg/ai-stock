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
- `GET /api/news/daily-outlooks`: 오늘 저장된 종목별 AI 전망 조회
- `POST /api/news/daily-outlooks`: 최근 24시간 뉴스 이벤트로 오늘의 AI 전망 생성
- `POST /api/news/daily-outlooks/market-data`: 전망에 현재가, 등락률, 거래량, 장중 변동성 결합
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

초기 전략 엔진은 현재가와 전일 대비 등락률을 받아 단순 규칙으로 판단합니다. 기본값은 추천 전용이며, 전일 대비 큰 하락은 매수 후보, 큰 상승은 매도 후보, 그 외는 관망으로 기록합니다.

뉴스 수집기는 기본 24시간 간격으로 실행되며 원문과 이미지는 저장하지 않습니다. 제목, 링크, 출처, 발행시각과 최대 500자의 피드 요약만 SQLite에 저장합니다. URL 기준 중복 제거, 하루 300건 제한, 90일 보관, DB 최대 1GB가 기본값이며 `.env`의 `NEWS_COLLECTION_INTERVAL_HOURS`, `NEWS_DAILY_LIMIT`, `NEWS_RETENTION_DAYS`, `NEWS_MAX_DATABASE_BYTES`로 조정할 수 있습니다.

수집 직후 Strategy 서비스는 OpenAI Structured Outputs로 뉴스 요약, 감성, 중요도, 영향 기간과 관련 종목을 분석합니다. 모델 호출 실패나 불완전 응답은 주문 신호로 사용하지 않고 실패 상태로 기록하며 최대 3회까지만 재시도합니다. 모델과 배치 크기는 `OPENAI_NEWS_MODEL`, `NEWS_ANALYSIS_BATCH_SIZE`로 설정합니다.

분석된 뉴스는 제목 핵심어, 관련 종목, 72시간 발행 구간을 기준으로 같은 사건끼리 묶습니다. 이벤트별 기사 수와 고유 출처 수를 분리하고, 종목별 감성은 기사 개수가 아니라 이벤트별 평균을 사용해 반복 보도가 신호를 부풀리지 않도록 합니다. 이벤트의 근거 신뢰도는 출처 다양성, 분석 완료율, 최고 중요도를 반영하며 매매 성공 확률을 의미하지 않습니다.

종목별 일일 AI 전망은 최근 24시간의 확인된 종목 뉴스 이벤트를 종합해 매수 후보, 매도 후보, 관망 중 하나와 신뢰도, 영향 기간, 핵심 근거를 생성합니다. 결과는 한국 시간 날짜별로 SQLite에 저장되며 화면 표시 전용입니다. 이 API는 주문 또는 자동매매 실행 코드를 호출하지 않습니다.

저장된 전망에는 KIS 현재가 응답의 현재가, 전일 대비 등락률, 누적 거래량과 장중 고가-저가 범위로 계산한 변동성을 결합할 수 있습니다. 매수 후보가 이미 전일 대비 5% 이상 상승했거나 장중 변동성이 10% 이상이면 차단 상태가 되며, AI가 정한 영향 기간이 지나면 만료됩니다. 이 상태 역시 주문을 실행하지 않고 이후 결정 엔진에서 사용할 사전 검증 결과로만 저장합니다.

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
