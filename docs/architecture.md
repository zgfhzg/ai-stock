# Architecture

## 목표

AI Stock은 Raspberry Pi에서 모의투자로 시작한 뒤 VPS로 이전할 수 있는 자동매매 시스템입니다. 핵심 주문 실행부는 Rust가 맡고, 전략 실험과 AI 판단은 Python이 맡습니다.

## 서비스

### Web

대시보드입니다. 계좌 상태, 보유 종목, 자동매매 상태, AI 제안, 주문/체결 로그를 보여줍니다.

### API

Rust 백엔드입니다.

- 한국투자증권 API 연동
- 접근 토큰 발급과 프로세스 내부 캐시
- 계좌, 잔고, 시세, 주문 API 제공
- 파일 기반 관심종목 관리
- JSONL 파일 기반 주문 로그
- 자동매매 상태 관리
- 주문 전 리스크 체크
- Strategy 서비스 호출

### Strategy

Python 서비스입니다.

- 전략 조건 평가
- AI 매매 제안 생성
- 백테스트
- 종목 스코어링

## 데이터 흐름

1. Web이 API에 현재 상태를 요청합니다.
2. API가 한국투자증권 API에서 계좌와 시세 정보를 조회합니다.
3. API가 Strategy에 판단 요청을 보냅니다.
4. Strategy는 추천과 근거를 반환합니다.
5. API는 리스크 규칙을 통과한 제안만 주문 후보로 저장합니다.
6. 실전 주문은 사용자가 명시적으로 허용한 경우에만 실행됩니다.

## 한국투자증권 API

초기 연동은 모의투자를 기준으로 합니다.

- 토큰 발급: `/oauth2/tokenP`
- 현재가 조회: `/uapi/domestic-stock/v1/quotations/inquire-price`, TR ID `FHKST01010100`
- 잔고 조회: `/uapi/domestic-stock/v1/trading/inquire-balance`, 모의 TR ID `VTTC8434R`

토큰 값은 API 응답이나 화면에 노출하지 않고, 서버 프로세스 내부에서만 보관합니다.

## 관심종목

초기 버전은 `data/watchlist.json` 파일에 관심종목을 저장합니다. 종목코드는 국내주식 6자리 숫자만 허용합니다. DB를 도입하기 전까지는 이 파일을 Raspberry Pi와 VPS 이전 시 같이 백업합니다.

## 주문

초기 주문 기능은 수동 모의투자 지정가 주문만 대상으로 합니다. 실전 모드에서는 `ENABLE_LIVE_TRADING=true`가 아니면 주문을 차단합니다. 주문금액은 `수량 × 지정가`로 계산하고 `MAX_ORDER_AMOUNT_KRW`를 초과하면 API에서 거절합니다.

주문 결과는 `data/orders.jsonl`에 한 줄 JSON 형식으로 기록합니다. 이 파일은 계좌 활동 로그에 해당하므로 Git에 포함하지 않습니다.

## 배포 전략

초기에는 Raspberry Pi에서 Docker Compose로 실행합니다. 이후 VPS로 옮길 때 같은 Compose 파일과 `.env` 설정을 사용합니다.
