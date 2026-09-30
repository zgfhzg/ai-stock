import os
import unittest
from unittest.mock import patch

from app.news_analysis import (
    ImpactHorizon,
    NewsAnalysis,
    NewsAnalysisBatch,
    NewsAnalysisRequest,
    NewsArticleInput,
    Sentiment,
    analyze_news,
)


class FakeResponses:
    def parse(self, **_kwargs):
        parsed = NewsAnalysisBatch(
            analyses=[
                NewsAnalysis(
                    article_id=7,
                    summary="반도체 투자가 확대됐습니다.",
                    sentiment=Sentiment.positive,
                    sentiment_score=0.6,
                    importance=3,
                    impact_horizon=ImpactHorizon.medium_term,
                    related_stocks=[],
                    rationale="투자 확대가 명시됐습니다.",
                )
            ]
        )
        return type("FakeResponse", (), {"output_parsed": parsed})()


class FakeClient:
    responses = FakeResponses()


class NewsAnalysisTests(unittest.TestCase):
    def setUp(self):
        self.request = NewsAnalysisRequest(
            articles=[
                NewsArticleInput(
                    id=7,
                    title="반도체 투자 확대",
                    source="테스트",
                    summary=None,
                )
            ]
        )

    def test_requires_api_key(self):
        with patch.dict(os.environ, {"OPENAI_API_KEY": ""}):
            with self.assertRaisesRegex(RuntimeError, "OPENAI_API_KEY"):
                analyze_news(self.request)

    def test_returns_validated_structured_output(self):
        with (
            patch.dict(os.environ, {"OPENAI_API_KEY": "test-key"}),
            patch("app.news_analysis.OpenAI", return_value=FakeClient()),
        ):
            response = analyze_news(self.request)

        self.assertEqual(response.analyses[0].article_id, 7)
        self.assertEqual(response.analyses[0].sentiment, Sentiment.positive)


if __name__ == "__main__":
    unittest.main()
