# Raspberry Pi 운영 가이드

이 문서는 Raspberry Pi에서 AI Stock을 로컬 앱처럼 실행하기 위한 운영 절차입니다.

## 권장 구성

- Raspberry Pi 4 이상 또는 Raspberry Pi 5
- 64-bit Raspberry Pi OS
- 외장 SSD 또는 충분한 여유 공간이 있는 microSD
- Docker Engine, Docker Compose plugin
- 같은 네트워크 또는 VPN에서 접속 가능한 고정 IP/호스트명

## 최초 설치

```sh
sudo apt update
sudo apt install -y git curl ca-certificates docker.io docker-compose-plugin
sudo usermod -aG docker $USER
```

권한 적용을 위해 한 번 로그아웃 후 다시 로그인합니다.

```sh
sudo mkdir -p /opt
sudo chown "$USER:$USER" /opt
git clone https://github.com/zgfhzg/ai-stock.git /opt/ai-stock
cd /opt/ai-stock
cp .env.example .env
```

`.env`에는 모의투자 키와 계좌를 먼저 넣습니다. 실전 주문은 기본값 그대로 꺼둡니다.

필수 값:

- `KIS_APP_KEY`
- `KIS_APP_SECRET`
- `KIS_ACCOUNT_NO`
- `KIS_ACCOUNT_PRODUCT_CODE`
- `KIS_BASE_URL`
- `AUTO_TRADE_MODE=recommend`
- `ENABLE_LIVE_TRADING=false`

## 실행

```sh
cd /opt/ai-stock
make up
make health
```

접속 주소:

- 대시보드: `http://라즈베리파이_IP:3000`
- API 상태: `http://라즈베리파이_IP:8080/health`
- Strategy 상태: `http://라즈베리파이_IP:8090/health`

대시보드는 접속한 호스트명을 기준으로 API 주소를 자동 계산합니다. 예를 들어 `http://192.168.0.20:3000`으로 열면 API는 `http://192.168.0.20:8080`을 사용합니다.

## 자동 시작

프로젝트를 `/opt/ai-stock`에 둔 경우:

```sh
sudo cp infra/systemd/ai-stock.service /etc/systemd/system/ai-stock.service
sudo systemctl daemon-reload
sudo systemctl enable ai-stock
sudo systemctl start ai-stock
sudo systemctl status ai-stock
```

프로젝트 경로가 다르면 `infra/systemd/ai-stock.service`의 `WorkingDirectory`를 실제 경로로 바꿉니다.

## 운영 명령

```sh
make ps       # 컨테이너 상태
make logs     # 실시간 로그
make restart  # 서비스 재시작
make down     # 전체 종료
make pull     # 원격 변경 가져오기
make up       # 다시 빌드 후 실행
```

## 업데이트

```sh
cd /opt/ai-stock
make down
make pull
make up
make health
```

## 데이터 위치

운영 데이터는 프로젝트의 `data` 폴더에 저장됩니다.

- 관심종목: `data/watchlist.json`
- 자동매매 규칙: `data/auto-rules.json`
- 리스크 설정: `data/risk-settings.json`
- 감시 로그: `data/auto-rule-checks.jsonl`
- 주문 로그: `data/orders.jsonl`
- KIS 토큰 캐시: `data/kis-token.json`

이 파일들은 민감하거나 로컬 상태에 가까우므로 Git에 올리지 않습니다.

## 안전 기본값

- `ENABLE_LIVE_TRADING=false`
- `AUTO_TRADE_MODE=recommend`
- 화면의 `모의 주문` 토글 기본 OFF
- 조건 충족 후 기본 10분 쿨다운

모의 자동주문을 테스트할 때만 `.env`에서 `AUTO_TRADE_MODE=paper_auto`로 바꾸고 서비스를 재시작합니다.
