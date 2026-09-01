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
docker compose up --build
```

서비스 주소:

- 대시보드: `http://127.0.0.1:3000`
- Rust API: `http://localhost:8080`
- Strategy API: `http://localhost:8090`

인앱 브라우저에서 `localhost:3000`이 이전 앱 화면을 보여주거나 계속 로딩되면 `http://127.0.0.1:3000`으로 접속합니다.

## 운영 원칙

- 실전 주문은 기본 OFF입니다.
- AI는 초기에는 주문 권한 없이 추천만 합니다.
- 모든 주문 판단과 API 응답은 로그로 남깁니다.
- 하루 손실 한도에 도달하면 자동매매를 멈춥니다.
- Raspberry Pi와 VPS 모두 같은 Docker Compose 구조로 실행합니다.
