import json
import os
from enum import Enum

from openai import OpenAI
from pydantic import BaseModel, Field


class Sentiment(str, Enum):
    positive = "positive"
    neutral = "neutral"
    negative = "negative"


class ImpactHorizon(str, Enum):
    intraday = "intraday"
    short_term = "short_term"
    medium_term = "medium_term"
    long_term = "long_term"
    unknown = "unknown"


class NewsArticleInput(BaseModel):
    id: int
    title: str
    source: str
    summary: str | None = None


class RelatedStock(BaseModel):
    name: str
    symbol: str
    relevance: float = Field(ge=0, le=1)


class NewsAnalysis(BaseModel):
    article_id: int
    summary: str
    sentiment: Sentiment
    sentiment_score: float = Field(ge=-1, le=1)
    importance: int = Field(ge=1, le=5)
    impact_horizon: ImpactHorizon
    related_stocks: list[RelatedStock]
    rationale: str


class NewsAnalysisBatch(BaseModel):
    analyses: list[NewsAnalysis]


class NewsAnalysisRequest(BaseModel):
    articles: list[NewsArticleInput] = Field(min_length=1, max_length=20)


class NewsAnalysisResponse(BaseModel):
    model: str
    analyses: list[NewsAnalysis]


SYSTEM_PROMPT = """
당신은 한국 주식시장 뉴스 분류기다. 입력 기사 제목과 짧은 피드 요약에 명시된 사실만 사용한다.
각 기사에 대해 한국어 한 문장 요약, 시장 영향 방향, 중요도, 영향 기간, 관련 종목을 구조화한다.
근거가 부족하면 neutral, importance 1, impact_horizon unknown을 사용한다.
기사에 회사명이 명시되지 않으면 관련 종목을 만들지 않는다. 종목코드를 확실히 알지 못하면 symbol은 빈 문자열로 둔다.
투자 권유나 매수·매도 지시는 만들지 않는다. article_id는 입력 값을 정확히 유지하고 모든 입력 기사에 결과 하나를 반환한다.
""".strip()


def analyze_news(request: NewsAnalysisRequest) -> NewsAnalysisResponse:
    api_key = os.getenv("OPENAI_API_KEY", "").strip()
    if not api_key:
        raise RuntimeError("OPENAI_API_KEY is not configured")

    model = os.getenv("OPENAI_NEWS_MODEL", "gpt-4o-mini").strip() or "gpt-4o-mini"
    client = OpenAI(api_key=api_key, timeout=45.0, max_retries=2)
    payload = [article.model_dump() for article in request.articles]
    response = client.responses.parse(
        model=model,
        input=[
            {"role": "system", "content": SYSTEM_PROMPT},
            {
                "role": "user",
                "content": "다음 뉴스 배열을 분석하세요.\n" + json.dumps(payload, ensure_ascii=False),
            },
        ],
        text_format=NewsAnalysisBatch,
    )
    parsed = response.output_parsed
    if parsed is None:
        raise RuntimeError("The model returned no structured news analysis")

    expected_ids = {article.id for article in request.articles}
    returned_ids = {analysis.article_id for analysis in parsed.analyses}
    if expected_ids != returned_ids or len(parsed.analyses) != len(request.articles):
        raise RuntimeError("The model response did not contain exactly one result per article")

    return NewsAnalysisResponse(model=model, analyses=parsed.analyses)
