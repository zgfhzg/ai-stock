import json
import os
from enum import Enum

from openai import OpenAI
from pydantic import BaseModel, Field

from app.news_analysis import ImpactHorizon, TokenUsage, extract_usage


class OutlookAction(str, Enum):
    buy_candidate = "buy_candidate"
    sell_candidate = "sell_candidate"
    hold = "hold"


class StockEventInput(BaseModel):
    event_key: str
    headline: str
    sentiment_score: float | None = None
    importance: int | None = Field(default=None, ge=1, le=5)
    impact_horizon: ImpactHorizon | None = None
    source_count: int = Field(ge=1)
    evidence_confidence: float = Field(ge=0, le=1)


class StockOutlookInput(BaseModel):
    symbol: str
    name: str
    events: list[StockEventInput] = Field(min_length=1, max_length=20)


class DailyOutlookRequest(BaseModel):
    stocks: list[StockOutlookInput] = Field(min_length=1, max_length=20)


class DailyStockOutlook(BaseModel):
    symbol: str
    action: OutlookAction
    confidence: float = Field(ge=0, le=1)
    impact_horizon: ImpactHorizon
    reasons: list[str] = Field(min_length=1, max_length=3)


class DailyOutlookBatch(BaseModel):
    outlooks: list[DailyStockOutlook]


class DailyOutlookResponse(BaseModel):
    model: str
    outlooks: list[DailyStockOutlook]
    usage: TokenUsage | None = None


SYSTEM_PROMPT = """
당신은 한국 주식시장 뉴스 이벤트를 종목별로 종합하는 분석기다.
입력에 포함된 사실만 사용하고 가격이나 재무정보를 추정하지 않는다.
각 종목마다 buy_candidate, sell_candidate, hold 중 하나를 반환한다.
상충하거나 근거가 약한 뉴스는 hold로 판단하고 confidence를 낮춘다.
반복 기사 수가 아니라 event_key별 사건과 source_count, evidence_confidence를 근거 품질로 사용한다.
reasons는 짧은 한국어 문장 1~3개로 작성한다. 주문 수량이나 실제 매매 지시는 만들지 않는다.
입력된 모든 symbol을 정확히 한 번씩 반환하고 다른 symbol을 추가하지 않는다.
""".strip()


def generate_daily_outlooks(request: DailyOutlookRequest) -> DailyOutlookResponse:
    api_key = os.getenv("OPENAI_API_KEY", "").strip()
    if not api_key:
        raise RuntimeError("OPENAI_API_KEY is not configured")

    model = os.getenv("OPENAI_NEWS_MODEL", "gpt-4o-mini").strip() or "gpt-4o-mini"
    client = OpenAI(api_key=api_key, timeout=45.0, max_retries=2)
    response = client.responses.parse(
        model=model,
        input=[
            {"role": "system", "content": SYSTEM_PROMPT},
            {
                "role": "user",
                "content": "다음 종목별 뉴스 이벤트를 종합하세요.\n"
                + json.dumps(request.model_dump(mode="json"), ensure_ascii=False),
            },
        ],
        text_format=DailyOutlookBatch,
    )
    parsed = response.output_parsed
    if parsed is None:
        raise RuntimeError("The model returned no structured daily outlook")

    expected = {stock.symbol for stock in request.stocks}
    returned = [outlook.symbol for outlook in parsed.outlooks]
    if set(returned) != expected or len(returned) != len(expected):
        raise RuntimeError("The model response did not contain exactly one outlook per stock")

    return DailyOutlookResponse(
        model=model,
        outlooks=parsed.outlooks,
        usage=extract_usage(response),
    )
