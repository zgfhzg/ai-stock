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

    return ProposalResponse(
        action="hold",
        confidence=0.1,
        reason=(
            f"{label} is in observation mode. "
            f"The current environment is {trading_mode}, and AI trading is recommendation-only."
        ),
    )
