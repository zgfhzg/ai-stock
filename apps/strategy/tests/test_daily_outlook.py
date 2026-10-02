import os
import unittest
from unittest.mock import patch

from app.daily_outlook import (
    DailyOutlookBatch,
    DailyOutlookRequest,
    DailyStockOutlook,
    OutlookAction,
    StockEventInput,
    StockOutlookInput,
    generate_daily_outlooks,
)
from app.news_analysis import ImpactHorizon


class FakeResponses:
    def parse(self, **_kwargs):
        parsed = DailyOutlookBatch(
            outlooks=[
                DailyStockOutlook(
                    symbol="005930",
                    action=OutlookAction.buy_candidate,
                    confidence=0.72,
                    impact_horizon=ImpactHorizon.short_term,
                    reasons=["반도체 공급 확대 뉴스의 근거 품질이 높습니다."],
                )
            ]
        )
        return type("FakeResponse", (), {"output_parsed": parsed, "usage": None})()


class FakeClient:
    responses = FakeResponses()


class DailyOutlookTests(unittest.TestCase):
    def setUp(self):
        self.request = DailyOutlookRequest(
            stocks=[
                StockOutlookInput(
                    symbol="005930",
                    name="삼성전자",
                    events=[
                        StockEventInput(
                            event_key="evt-1",
                            headline="HBM 공급 확대",
                            sentiment_score=0.6,
                            importance=4,
                            impact_horizon=ImpactHorizon.short_term,
                            source_count=2,
                            evidence_confidence=0.8,
                        )
                    ],
                )
            ]
        )

    def test_returns_one_outlook_per_requested_stock(self):
        with (
            patch.dict(os.environ, {"OPENAI_API_KEY": "test-key"}),
            patch("app.daily_outlook.OpenAI", return_value=FakeClient()),
        ):
            response = generate_daily_outlooks(self.request)
        self.assertEqual(response.outlooks[0].symbol, "005930")
        self.assertEqual(response.outlooks[0].action, OutlookAction.buy_candidate)


if __name__ == "__main__":
    unittest.main()
