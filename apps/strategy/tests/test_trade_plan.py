import os
import unittest
from unittest.mock import patch

from pydantic import ValidationError

from app.trade_plan import TradePlan, TradePlanRequest, generate_trade_plan


class TradePlanTests(unittest.TestCase):
    def setUp(self):
        self.request = TradePlanRequest(
            symbol="005930", name="삼성전자", current_price=70000,
            quote={"acml_vol": "0"}, news={"reasons": ["공급 계약"]},
            holding_quantity=0, average_purchase_price=0,
            maximum_buy_amount_krw=100000, maximum_order_amount_krw=100000,
        )
        self.plan = dict(
            symbol="005930", action="buy", confidence=0.8,
            entry_price=69000, take_profit_price=75000, stop_loss_price=66000,
            quantity=1, valid_for_minutes=60, reason="뉴스 근거로 지정가 진입",
        )

    def generate(self, plan):
        parsed = TradePlan(**plan)
        response = type("Response", (), {"output_parsed": parsed, "usage": None})()
        with patch.dict(os.environ, {"OPENAI_API_KEY": "test"}), patch("app.trade_plan.OpenAI") as client:
            client.return_value.responses.parse.return_value = response
            result = generate_trade_plan(self.request)
            self.assertEqual(client.call_args.kwargs["max_retries"], 0)
            return result

    def test_low_volume_is_context_not_an_automatic_rejection(self):
        result = self.generate(self.plan)
        self.assertEqual(result.plan.entry_price, 69000)

    def test_ai_can_choose_opening_auction(self):
        result = self.generate(self.plan | {"session": "opening_auction"})
        self.assertEqual(result.plan.session, "opening_auction")

    def test_pending_review_cannot_increase_remaining_quantity(self):
        self.request.maximum_buy_amount_krw = 300000
        self.request.maximum_order_amount_krw = 300000
        self.request.pending_order = {"side": "buy", "remaining_quantity": 1}
        with self.assertRaises(RuntimeError):
            self.generate(self.plan | {"quantity": 2})

    def test_partial_buy_review_is_limited_to_the_unfilled_order(self):
        self.request.holding_quantity = 1
        self.request.pending_order = {"side": "buy", "remaining_quantity": 1}
        result = self.generate(self.plan)
        self.assertEqual(result.plan.quantity, 1)

    def test_rejects_unavailable_session(self):
        self.request.available_sessions = ["regular"]
        with self.assertRaises(RuntimeError):
            self.generate(self.plan | {"session": "opening_auction"})

    def test_rejects_reversed_exit_prices(self):
        with self.assertRaises(ValidationError):
            TradePlan(**(self.plan | {"stop_loss_price": 70000}))

    def test_rejects_budget_and_symbol_mismatch(self):
        for changes in ({"quantity": 2}, {"symbol": "000660"}):
            with self.subTest(changes=changes), self.assertRaises(RuntimeError):
                self.generate(self.plan | changes)

    def test_cannot_sell_unheld_shares(self):
        with self.assertRaises(RuntimeError):
            self.generate(self.plan | {"action": "sell"})

    def test_cannot_discard_protection_on_a_held_position(self):
        self.request.holding_quantity = 1
        self.request.previous_plan = self.plan
        with self.assertRaises(RuntimeError):
            self.generate(self.plan | {"action": "hold", "quantity": 0, "stop_loss_price": 0, "take_profit_price": 0})


if __name__ == "__main__":
    unittest.main()
