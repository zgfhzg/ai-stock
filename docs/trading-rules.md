# Trading Rules

## 기본 원칙

- 기본 모드는 모의투자입니다.
- 실전 주문은 `ENABLE_LIVE_TRADING=true`일 때만 허용합니다.
- AI는 초기에는 추천만 생성합니다.
- `AUTO_TRADE_MODE=recommend`에서는 AI 판단만 기록하고 주문하지 않습니다.
- `AUTO_TRADE_MODE=paper_auto`와 실행 요청이 함께 들어온 경우에만 모의 자동 주문을 시도합니다.
- 주문 실행 전에는 항상 리스크 체크를 통과해야 합니다.

## 리스크 제한

- 1회 주문 최대 금액: `MAX_ORDER_AMOUNT_KRW`
- 종목별 최대 비중: `MAX_POSITION_RATIO`
- 하루 최대 손실률: `DAILY_MAX_LOSS_RATIO`
- 하루 최대 주문 횟수: `DAILY_MAX_ORDER_COUNT`

## 자동 중지 조건

- 하루 손실률 초과
- 한국투자증권 API 인증 실패 반복
- 주문 실패 반복
- 시세 데이터 지연
- Strategy 서비스 응답 실패

## AI 주문 권한

초기 버전에서 AI는 직접 주문하지 않습니다. AI 제안은 사용자가 승인하거나, 사전에 허용된 전략 규칙과 금액 제한을 통과해야만 주문 후보가 됩니다.
