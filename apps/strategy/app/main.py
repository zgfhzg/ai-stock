import os

from fastapi import FastAPI
from pydantic import BaseModel


app = FastAPI(title="AI Stock Strategy")


class HealthResponse(BaseModel):
    status: str
    service: str


class ProposalRequest(BaseModel):
    symbol: str
    name: str | None = None
    current_price: int | None = None
    previous_change: int | None = None
    previous_change_rate: float | None = None


class ProposalResponse(BaseModel):
    action: str
    confidence: float
    reason: str
    live_order_allowed: bool = False


@app.get("/health")
def health() -> HealthResponse:
    return HealthResponse(status="ok", service="strategy")


@app.post("/strategy/proposal")
def create_proposal(request: ProposalRequest) -> ProposalResponse:
    trading_mode = os.getenv("TRADING_MODE", "paper")
    label = request.name or request.symbol
    rate = request.previous_change_rate

    if rate is None or request.current_price is None:
        return ProposalResponse(
            action="hold",
            confidence=0.2,
            reason=f"{label}: 현재가 데이터가 부족해서 관망합니다.",
        )

    if rate <= -4.0:
        return ProposalResponse(
            action="buy",
            confidence=0.72,
            reason=(
                f"{label}: 전일 대비 {rate:.2f}% 하락했습니다. "
                "단기 과매도 후보로 관찰 매수 신호를 냅니다."
            ),
        )

    if rate >= 5.0:
        return ProposalResponse(
            action="sell",
            confidence=0.68,
            reason=(
                f"{label}: 전일 대비 {rate:.2f}% 상승했습니다. "
                "급등 구간이라 차익실현 후보로 봅니다."
            ),
        )

    if -1.0 <= rate <= 1.0:
        return ProposalResponse(
            action="hold",
            confidence=0.55,
            reason=f"{label}: 전일 대비 변동이 {rate:.2f}%로 작아 관망합니다.",
        )

    return ProposalResponse(
        action="hold",
        confidence=0.45,
        reason=(
            f"{label}: 전일 대비 {rate:.2f}% 움직였습니다. "
            f"{trading_mode} 환경에서는 아직 추가 확인 후 관망합니다."
        ),
    )
